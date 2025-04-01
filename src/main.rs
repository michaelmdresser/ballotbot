use log::{debug, error, info, warn};
use serenity::async_trait;
// use serenity::futures::AsyncReadExt;
use serenity::builder::CreateMessage;
use serenity::model::prelude::*;
use serenity::prelude::*;
use std::collections::BTreeMap;
use std::time::SystemTime;
mod condorcet_voting;
use crate::condorcet_voting::cv;

struct Bot {
    database: sqlx::SqlitePool,
}

// newsession (n)
// participate (p)
// suggest (s)
// vote (v)

impl Bot {
    async fn option_id_to_option(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        session_id: i64,
    ) -> BTreeMap<u32, String> {
        let rows = sqlx::query!(
            "SELECT option_name, option_id FROM session_ballot_options WHERE session_id = ? ORDER BY ROWID",
            session_id,
        )
        .fetch_all(&mut **tx)
        .await
        .unwrap();

        let option_id_to_option: BTreeMap<u32, String> =
            rows.into_iter().fold(BTreeMap::new(), |mut map, row| {
                let option_name = row.option_name;
                let option_id = row.option_id;
                map.insert(u32::try_from(option_id).unwrap(), option_name);
                return map;
            });
        return option_id_to_option;
    }
    async fn option_to_option_id(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        session_id: i64,
    ) -> BTreeMap<String, u32> {
        let rows  = sqlx::query!(
            "SELECT option_name, option_id FROM session_ballot_options WHERE session_id = ? ORDER BY ROWID",
            session_id,
        )
        .fetch_all(&mut **tx)
        .await
        .unwrap();

        let option_to_option_id: BTreeMap<String, u32> =
            rows.into_iter().fold(BTreeMap::new(), |mut map, row| {
                let option_name = row.option_name;
                let option_id = row.option_id;
                map.insert(option_name, u32::try_from(option_id).unwrap());
                return map;
            });
        return option_to_option_id;
    }

    async fn is_voting_complete(
        &self,
        vote_tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        session_id: i64,
    ) -> bool {
        let voters_left = sqlx::query!(
            "
SELECT COUNT(user) AS c FROM (
  SELECT participant AS user
  FROM session_participants
  WHERE session_id = ?
  EXCEPT
  SELECT DISTINCT voter AS user
  FROM session_votes
  WHERE session_id = ?
)
",
            session_id,
            session_id,
        )
        .fetch_one(&mut **vote_tx)
        .await
        .unwrap()
        .c;

        return voters_left == 0;
    }

    async fn finish_vote(
        &self,
        vote_tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        ctx: &Context,
        session_id: i64,
        channel: ChannelId,
    ) {
        // FIXME: Close session after done.
        // TODO: transaction

        let votes = sqlx::query!(
            r#"
SELECT voter, ballot_option_id, rank
FROM session_votes
WHERE session_id = ?
GROUP BY session_id, voter, ballot_option_id, rank
ORDER BY voter, rank ASC
"#,
            session_id,
        )
        .fetch_all(&mut **vote_tx)
        .await
        .unwrap();

        sqlx::query!(
            "UPDATE voting_session SET status = 'finished' WHERE id = ?",
            session_id,
        )
        .execute(&mut **vote_tx)
        .await
        .unwrap();

        let option_to_option_id = self.option_to_option_id(vote_tx, session_id).await;
        let option_id_to_option = self.option_id_to_option(vote_tx, session_id).await;

        let ballots_by_user: BTreeMap<String, cv::Ballot> = votes.iter().fold(
            BTreeMap::new(),
            |mut map: BTreeMap<String, cv::Ballot>, row| {
                let option_id: u32 = u32::try_from(row.ballot_option_id).unwrap();
                let voter: String = row.voter.clone();
                let mut ballot: cv::Ballot = match map.get(&voter) {
                    Some(ballot) => ballot.to_vec(),
                    None => cv::Ballot::new(),
                };
                // TODO: Assert that ballot position matches row ordering
                ballot.push(option_id);
                map.insert(voter, ballot);
                return map;
            },
        );
        let ballots: Vec<cv::Ballot> = ballots_by_user.into_values().collect();
        info!("Ballots for finish_vote: {:?}", ballots);

        let result = cv::vote(u32::try_from(option_to_option_id.len()).unwrap(), ballots).unwrap();
        let result_cloned = result.clone();

        let response: String = match (
            result_cloned.win_type,
            result_cloned.winner,
            result_cloned.final_outranking,
            result_cloned.schulze_result,
        ) {
            (Some(cv::WinType::CondorcetWinner), Some(winner), overall_outranking, _) => {
                // clean condorcet winner
                info!("Win via condorcet. Result:\n{}", result);
                format!(
                    "Condorcet winner (unambiguous): {winner} - **{}**.
Outranking matrix: ```{}```",
                    option_id_to_option.get(&winner).unwrap(),
                    overall_outranking,
                )
            }
            (
                Some(cv::WinType::SchulzeRanking),
                Some(winner),
                overall_outranking,
                Some(schulze_result),
            ) => {
                let ranking_str_untrimmed: String = schulze_result
                    .0
                    .into_iter()
                    .fold("".to_string(), |acc: String, (choice, _)| {
                        acc + &format!("{} > ", choice)
                    });
                let ranking_str = ranking_str_untrimmed
                    .strip_suffix(" > ")
                    .unwrap_or(&ranking_str_untrimmed);
                info!(
                    "Win via Schulze. Result:\n{}\nRanking: {ranking_str}",
                    result,
                );
                format!(
                    "Winner via Schulze method: {winner} - {}.\nSchulze ranking: {ranking_str}\nBase outranking matrix: ```{}```",
                    option_id_to_option.get(&winner).unwrap(), overall_outranking,
                )
            }
            (_, _, _, _) => {
                error!("Unsupported vote result: {:?}", result);
                format!("Unsupported vote result: {:?}", result)
            }
        };

        match channel.say(&ctx, response).await {
            Ok(_) => {}
            Err(err) => {
                error!("Failed to send vote-result message: {err}");
            }
        };
    }

    async fn latest_guild_session(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        ctx: &Context,
        guild: String,
        channel_id: ChannelId,
    ) -> Option<i64> {
        match sqlx::query!(
                "SELECT id FROM voting_session WHERE discord_server = ? AND status = 'open' ORDER BY id LIMIT 1",
                guild,
        )
        .fetch_one(&mut **tx)
        .await {
            Ok(row) => return Some(row.id),
            Err(err) => {
                info!("No open voting_sesssion found for server {guild}: {err}");
                let response = format!("Failed to find an open voting session for your server. Try `^newsession`");

                match channel_id.say(&ctx, response).await {
                    Ok(_) => {},
                    Err(err) => error!("Failed to send no-open-session response: {err}"),
                };
                return None
            },
        };
    }

    async fn is_participating(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        session_id: i64,
        user: String,
    ) -> bool {
        match sqlx::query!(
            "SELECT COUNT(*) AS ct FROM session_participants WHERE session_id = ? AND participant = ?", session_id, user,
        )
            .fetch_one(&mut **tx)
            .await {
                Ok(row) => {
                    if row.ct == 0 {
                        return false
                    } else if row.ct == 1 {
                        return true
                    } else {
                        error!("Unexpected count of participants for session_id: {session_id} and user: {user}. This should not happen.");
                        return false
                    }
                },
                Err(err) => {
                    error!("Failed to query for participating: {err}");
                    return false
                },
            };
    }
}

use std::error::Error;
use std::fmt;

// TODO: Better error...
#[derive(Debug)]
pub struct ParseError {
    details: String,
}

impl ParseError {
    fn new(msg: &str) -> ParseError {
        ParseError {
            details: msg.to_string(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl Error for ParseError {
    fn description(&self) -> &str {
        &self.details
    }
}

// TODO: Better error...
#[derive(Debug)]
pub struct ParticipateError {
    details: String,
}

impl ParticipateError {
    fn new(msg: &str) -> ParticipateError {
        ParticipateError {
            details: msg.to_string(),
        }
    }
}

impl fmt::Display for ParticipateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.details)
    }
}

impl Error for ParticipateError {
    fn description(&self) -> &str {
        &self.details
    }
}

fn parse_cv_ballot(ballot_str: String) -> Result<cv::Ballot, ParseError> {
    let mut ballot: cv::Ballot = Vec::new();
    let raw_choices = ballot_str.trim().split(">");
    for choice_raw in raw_choices.into_iter() {
        let choice_parsed = match choice_raw.trim().parse::<u32>() {
            Ok(r) => r,
            Err(err) => {
                return Err(ParseError::new(&format!(
                    "Vote '{choice_raw}' could not be parsed to u32: {err}",
                )));
            }
        };
        ballot.push(choice_parsed);
    }

    let mut ballot_sorted: cv::Ballot = ballot.clone();
    ballot_sorted.sort();
    ballot_sorted.dedup();
    if ballot_sorted.len() != ballot.len() {
        return Err(ParseError::new(
            &format!("Bad ballot: duplicates detected",),
        ));
    }

    return Ok(ballot);
}

#[test]
fn test_parse_cv_ballot() {
    struct Case {
        input: String,
        expected: cv::Ballot,
    }

    let cases = [
        Case {
            input: "1 > 3 > 2".to_string(),
            expected: cv::Ballot::from([1, 3, 2]),
        },
        Case {
            input: "  3 > 2 >1  \n".to_string(),
            expected: cv::Ballot::from([3, 2, 1]),
        },
    ];

    for case in cases.iter() {
        assert_eq!(parse_cv_ballot(case.input.clone()).unwrap(), case.expected,);
    }
}

#[async_trait]
impl EventHandler for Bot {
    async fn ready(&self, _: Context, ready: Ready) {
        info!("Connected as {}", ready.user.name);
    }

    async fn message(&self, ctx: Context, msg: Message) -> () {
        let chan_respond = async |to_send: &str| -> () {
            let msg_to_send = CreateMessage::new().content(format!(
                "{}: {to_send}",
                msg.author_nick(&ctx).await.unwrap_or("UNKNOWN".to_string())
            ));
            if let Err(say_err) = msg.channel_id.send_message(&ctx, msg_to_send).await {
                error!(
                    "Failed to responsd to channel_id {}: {say_err}",
                    msg.channel_id
                );
            }
            ()
        };
        let dm_respond = async |to_send: &str| -> () {
            let msg_to_send = CreateMessage::new().content(to_send);
            if let Err(say_err) = msg.author.direct_message(&ctx, msg_to_send).await {
                error!("Failed to respond to author {}: {say_err}", msg.author);
            }
            ()
        };
        let participate = async |tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
                                 session_id: i64,
                                 user: &str|
               -> Result<(), ParticipateError> {
            match sqlx::query!(
                "INSERT INTO session_participants (session_id, participant) VALUES (?, ?)
                   ON CONFLICT(session_id, participant) DO UPDATE SET session_id=excluded.session_id
",
                session_id,
                user,
            )
            .execute(&mut **tx)
            .await
            {
                Ok(_) => {}
                Err(err) => {
                    error!("Failed to insert into session_participants. session_id: {session_id}. participant: {user}. Err: {err}");
                    chan_respond("Failed to add you to the participants of the voting session.")
                        .await;
                    return Err(ParticipateError {
                        details: format!("{err}"),
                    });
                }
            };

            chan_respond("You are participating in the current session").await;
            return Ok(());
        };

        // TODO: Maybe not strip_prefix?
        // TODO: Configurable prefix
        // TODO: Shorthand ("n")
        // if let Some(_) = msg.content.strip_prefix("^newsession") {
        //
        /////////////////////////////////////////////////////////////
        // ^newsession
        /////////////////////////////////////////////////////////////
        if msg.content.eq("^newsession") {
            let guild = match msg.guild_id {
                Some(id) => id.to_string(),
                None => {
                    info!("Received new session message with no guild ID");
                    return;
                }
            };
            let channel = msg.channel_id.to_string();

            let mut newsession_tx = self.database.begin().await.unwrap();
            match sqlx::query!(
                "UPDATE voting_session SET status = 'closed_new' WHERE discord_server = ? AND server_channel = ?",
                guild, channel,
            ).execute(&mut *newsession_tx).await {
                Ok(_) => {}
                Err(err) => {
                    error!("Failed to update old sessions: {err}");
                    chan_respond("Failed to make a new session.").await;
                    newsession_tx.rollback().await.unwrap();
                    return;
                }
            };

            match sqlx::query!(
                "INSERT INTO voting_session (discord_server, server_channel, status) VALUES (?,?, 'open')",
                guild,
                channel,
            )
            .execute(&mut *newsession_tx)
            .await
            {
                Ok(_) => {
                    info!("Started voting_session. Server: {guild}. Channel: {channel}");
                }
                Err(err) => {
                    error!("Failed to insert voting_session: {err}");
                    chan_respond("Failed to make a new session.").await;
                    newsession_tx.rollback().await.unwrap();
                    return;
                }
            };

            debug!("Committing new session");
            newsession_tx.commit().await.unwrap();
            chan_respond("Started new voting session. Make sure to `^suggest` a candidate!").await;
        }
        /////////////////////////////////////////////////////////////
        // ^participate
        /////////////////////////////////////////////////////////////
        else if msg.content.eq("^participate") {
            let guild = match msg.guild_id {
                Some(id) => id.to_string(),
                None => {
                    warn!("Received participate message with no guild ID");
                    return;
                }
            };
            let user: String = msg.author.id.get().to_string();

            let mut participate_tx = match self.database.begin().await {
                Ok(tx) => tx,
                Err(err) => {
                    error!("Failed to start participate tx: {err}");
                    chan_respond("Failed to participate.").await;
                    return;
                }
            };

            let latest_guild_session = match self
                .latest_guild_session(&mut participate_tx, &ctx, guild.clone(), msg.channel_id)
                .await
            {
                Some(session) => session,
                None => {
                    error!(
                        "Failed to get latest_guild_session for guild {}, channel {}",
                        guild.clone(),
                        msg.channel_id
                    );
                    chan_respond("Failed to participate").await;
                    participate_tx.rollback().await.unwrap();
                    return;
                }
            };

            if let Err(err) = participate(&mut participate_tx, latest_guild_session, &user).await {
                error!("Failed to participate({latest_guild_session}, user): {err}");
                chan_respond("Failed to participate").await;
                participate_tx.rollback().await.unwrap();
                return;
            }

            if let Err(err) = participate_tx.commit().await {
                error!("Failed to commit participate tx: {err}");
                chan_respond("Failed to commit participate tx. Something is wrong.").await;
                return;
            };
        }
        /////////////////////////////////////////////////////////////
        // ^suggest
        /////////////////////////////////////////////////////////////
        else if let Some(suggestion) = msg.content.strip_prefix("^suggest") {
            let suggestion = suggestion.trim();
            let guild = match msg.guild_id {
                Some(id) => id.to_string(),
                None => {
                    info!("Received suggest message with no guild ID");
                    return;
                }
            };

            if suggestion.trim().len() == 0 {
                chan_respond("Cannot vote with an empty string").await;
                return;
            }

            let user: String = msg.author.id.get().to_string();

            let mut option_insert_tx: sqlx::Transaction<'_, sqlx::Sqlite> =
                match self.database.begin().await {
                    Ok(tx) => tx,
                    Err(err) => {
                        error!("Failed to start transaction for suggestion insert: {err}");
                        chan_respond("Failed to suggest").await;
                        return;
                    }
                };

            let latest_guild_session = match self
                .latest_guild_session(&mut option_insert_tx, &ctx, guild.clone(), msg.channel_id)
                .await
            {
                Some(session) => session,
                None => {
                    error!(
                        "Failed to get latest_guild_session for guild {}, channel {}",
                        guild.clone(),
                        msg.channel_id
                    );
                    chan_respond("Failed to suggest").await;
                    option_insert_tx.rollback().await.unwrap();
                    return;
                }
            };

            if !self
                .is_participating(&mut option_insert_tx, latest_guild_session, user.clone())
                .await
            {
                debug!("User {user} is not yet participating in session {latest_guild_session} for guild {guild}. Adding.");
                if let Err(err) =
                    participate(&mut option_insert_tx, latest_guild_session, &user).await
                {
                    error!("Failed to participate({latest_guild_session}, user): {err}");
                    chan_respond("Failed to participate").await;
                    option_insert_tx.rollback().await.unwrap();
                    return;
                }
            }

            let _ = match sqlx::query!(
                "SELECT participant
                 FROM session_participants
                 WHERE session_id = ? AND participant = ?
                 ORDER BY session_id LIMIT 1",
                latest_guild_session,
                user,
            )
            .fetch_one(&mut *option_insert_tx)
            .await
            {
                Ok(row) => row,
                Err(err) => {
                    error!("Failed to get participant in suggest. session_id: {latest_guild_session}. participant: {user}. Err: {err}");
                    chan_respond("Failed to suggest.").await;
                    option_insert_tx.rollback().await.unwrap();
                    return
                }
            }
            .participant;

            let suggestion_id: u32 = match sqlx::query!(
                "
WITH max_option AS (
    SELECT MAX(option_id) AS max_id
    FROM session_ballot_options
    WHERE session_id = ?
),
suggester_option AS (
    SELECT option_id
    FROM session_ballot_options
    WHERE session_id = ? AND suggester = ?
)
SELECT
    CASE
        WHEN (SELECT COUNT(*) FROM session_ballot_options WHERE session_id = ?) = 0 THEN 0
        WHEN EXISTS (SELECT 1 FROM suggester_option) THEN (SELECT option_id FROM suggester_option)
        ELSE (SELECT COALESCE(max_id + 1, -1) FROM max_option)
    END AS result;
",
                latest_guild_session,
                latest_guild_session,
                user,
                latest_guild_session,
            )
            .fetch_one(&mut *option_insert_tx)
            .await
            {
                Ok(row) => u32::try_from(row.result.unwrap()).unwrap(),
                Err(err) => {
                    error!(
                        "Failed to get next option ID for session {latest_guild_session}: {err}"
                    );
                    chan_respond("Failed to suggest.").await;
                    option_insert_tx.rollback().await.unwrap();
                    return;
                }
            };

            match sqlx::query!(
                "INSERT INTO session_ballot_options (session_id, suggester, option_id, option_name)
                 VALUES (?, ?, ?, ?)
                   ON CONFLICT (session_id, suggester, option_id) DO UPDATE SET option_name=excluded.option_name
",
                latest_guild_session,
                user,
                suggestion_id,
                suggestion,
            )
            .execute(&mut *option_insert_tx)
            .await {
                Ok(_) => {},
                Err(err) => {
                    error!("Failed to update ballot option. session_id: {latest_guild_session}. suggester: {user}. ballot_option: {suggestion}. Err: {err}");
                    chan_respond("Failed to suggest.").await;
                    option_insert_tx.rollback().await.unwrap();
                    return;
                },
            };

            match option_insert_tx.commit().await {
                Ok(_) => {
                    info!("Inserted suggested for {suggestion} as ID {suggestion_id}");
                }
                Err(err) => {
                    error!("Failed tx commit for suggestion: {err}");
                    chan_respond("Failed to suggest.").await;
                    return;
                }
            }

            chan_respond(&format!("Suggested `{}`", suggestion)).await;
            debug!("Finished suggest")
        }
        /////////////////////////////////////////////////////////////
        // ^vote
        /////////////////////////////////////////////////////////////
        else if msg.content.eq("^vote") {
            // TODO(michael.dresser): Figure out how to deal with the same user being in two different active voting sessions. Either don't allow or override?

            let guild = match msg.guild_id {
                Some(id) => id.to_string(),
                None => {
                    info!("Received vote message with no guild ID");
                    return;
                }
            };

            let mut tx = self.database.begin().await.unwrap();

            let latest_guild_session = match self
                .latest_guild_session(&mut tx, &ctx, guild.clone(), msg.channel_id)
                .await
            {
                Some(session) => session,
                None => {
                    error!(
                        "Failed to get latest_guild_session for guild {}, channel {}",
                        guild.clone(),
                        msg.channel_id
                    );
                    chan_respond("Failed to start vote").await;
                    tx.rollback().await.unwrap();
                    return;
                }
            };

            match sqlx::query!(
                "UPDATE voting_session SET status = 'voting' WHERE id = ?",
                latest_guild_session,
            )
            .execute(&mut *tx)
            .await
            {
                Ok(_) => info!("Started voting for session {latest_guild_session}"),
                Err(err) => {
                    error!("Failed to start voting for session {latest_guild_session}: {err}");
                    chan_respond("Failed to start vote").await;
                    tx.rollback().await.unwrap();
                    return;
                }
            };

            let mut ballot_message = format!("Ballot options:\n");
            for (id, option) in self
                .option_id_to_option(&mut tx, latest_guild_session)
                .await
            {
                ballot_message += &format!("{}: {}\n", id, option);
            }
            ballot_message += "\nExample response: `3 > 1 > 2 > 0`";

            let session_participants = match sqlx::query!(
                "SELECT participant FROM session_participants WHERE session_id = ?",
                latest_guild_session,
            )
            .fetch_all(&mut *tx)
            .await
            {
                Ok(x) => x,
                Err(err) => {
                    error!(
                        "Failed to get session participants for id {latest_guild_session}: {err}"
                    );

                    chan_respond("Failed to start vote").await;
                    tx.rollback().await.unwrap();
                    return;
                }
            };

            for row in session_participants.iter() {
                let user = UserId::new(row.participant.to_string().parse::<u64>().unwrap());
                match user
                    .direct_message(&ctx, CreateMessage::new().content(ballot_message.clone()))
                    .await
                {
                    Ok(_) => {
                        info!("Sent ballot to {0}", user.to_string())
                    }
                    Err(err) => {
                        error!("Failed to send ballot to {0}: {err}", user.to_string());
                        chan_respond("Failed to start vote").await;
                        tx.rollback().await.unwrap();
                        return;
                    }
                };
            }

            chan_respond(&format!(
                "Ballots sent to participants. Ballot message:\n{}",
                ballot_message
            ))
            .await;

            if let Err(err) = tx.commit().await {
                error!("Failed to commit ^vote tx: {err}");
                chan_respond("Voting probably didn't start due to an unexpected error").await;
                return;
            }

            info!("Finished sending ballots for session {latest_guild_session}")
        }
        /////////////////////////////////////////////////////////////
        // DM
        /////////////////////////////////////////////////////////////
        else if msg.guild_id.is_none() {
            // This should be a DM, meaning its a vote

            if msg.author.id == ctx.cache.current_user().id {
                return;
            }

            // TODO probably a vote, but maybe we should check a prefix at somepoint
            let user = msg.author.id.get().to_string();

            let ballot: cv::Ballot = match parse_cv_ballot(msg.content.clone()) {
                Ok(b) => b,
                Err(err) => {
                    error!("Failed to parse ballot {0}: {err}", msg.content.clone());

                    dm_respond(&format!("Failed to parse your ballot: {err}")).await;
                    return;
                }
            };

            debug!("Starting vote tx");

            let mut vote_tx = self.database.begin().await.unwrap();

            // TODO: This will almost definitely break for people participating in multiple
            // guilds.
            let sessions_in_voting_state: Vec<_> = match sqlx::query!(
                "SELECT s.id, s.discord_server, s.server_channel
                 FROM voting_session s
                 INNER JOIN session_participants p
                   ON s.id = p.session_id
                 WHERE s.status = 'voting'
                   AND p.participant = ?
                 ORDER BY id",
                user
            )
            .fetch_all(&mut *vote_tx)
            .await
            {
                Ok(x) => x,
                Err(err) => {
                    error!("Failed to query active sessions: {err}");
                    dm_respond("Failed to find an active voting session for you").await;
                    vote_tx.rollback().await.unwrap();
                    return;
                }
            }
            .into_iter()
            .collect();

            debug!("Queried active sessions");

            if sessions_in_voting_state.len() == 0 {
                error!(
                    "No sessions in voting state for DMing user {}.",
                    user.to_string()
                );
                dm_respond("You are trying to vote but you don't appear to be in any active voting sessions.").await;
                vote_tx.rollback().await.unwrap();
                return;
            } else if sessions_in_voting_state.len() > 1 {
                error!(
                    "Found more than 1 session in voting state for DMing user {}",
                    user.to_string()
                );
                dm_respond(&format!("You are trying to vote but you appear to be in {} (> 1) voting sessions. Contact the admin.", sessions_in_voting_state.len())).await;
                vote_tx.rollback().await.unwrap();
                return;
            }

            debug!("Got session");

            let row = sessions_in_voting_state.get(0).unwrap();
            let session_channel = ChannelId::new(row.server_channel.parse::<u64>().unwrap());
            let session_id = row.id.unwrap();

            // TODO: Include options in parsing?
            let option_id_to_option = self.option_id_to_option(&mut vote_tx, session_id).await;

            // TODO: parse, don't validate
            for ballot_entry in ballot.iter() {
                if !option_id_to_option.contains_key(&ballot_entry) {
                    dm_respond(&format!(
                        "Your ballot contains in invalid key: {ballot_entry}"
                    ))
                    .await;
                    vote_tx.rollback().await.unwrap();
                    return;
                }
            }

            debug!("Verified ballot keys");

            let author_id = msg.author.id.get().to_string();

            // TODO: transaction
            for (rank, option_id) in ballot.iter().enumerate() {
                let rank_i64: i64 = i64::try_from(rank).unwrap();
                match sqlx::query!(
                    "INSERT INTO session_votes (session_id, voter, ballot_option_id, rank)
                     VALUES (?,?,?,?)
                     ON CONFLICT(session_id, voter, ballot_option_id)
                       DO UPDATE SET rank=excluded.rank
",
                    session_id,
                    author_id,
                    option_id,
                    rank_i64,
                )
                .execute(&mut *vote_tx)
                .await
                {
                    Ok(_) => {}
                    Err(err) => {
                        error!("Failed to insert vote: {err}");
                        dm_respond(&format!("Failed to insert your vote: {err}")).await;
                        vote_tx.rollback().await.unwrap();
                        return;
                    }
                };
            }

            debug!("Inserted votes");

            dm_respond(&format!("Ballot recorded: {:?}", ballot)).await;

            if self.is_voting_complete(&mut vote_tx, session_id).await {
                self.finish_vote(&mut vote_tx, &ctx, session_id, session_channel)
                    .await;
            }

            if let Err(err) = vote_tx.commit().await {
                error!("Failed to commit DM vote tx: {err}");
                chan_respond("Failed to commit your vote tx. Something is wrong.").await;
                return;
            }
            debug!("Committed vote tx");
        }
        /////////////////////////////////////////////////////////////
        // Catch-all / help
        /////////////////////////////////////////////////////////////
        else if msg.content.starts_with("^") {
            chan_respond("Unknown command.
Options:
- `^newsession`: Starts a new voting session.
- `^participate`: Join the voting session.
- `^suggest`: Add a candidate to the voting session. Max of one candidate per user. Auto-participates.
- `^vote`: Start voting. Once all participants have responded, the result will be posted to this channel.
").await;
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let colors = fern::colors::ColoredLevelConfig::new()
        .debug(fern::colors::Color::Blue)
        .info(fern::colors::Color::Green);
    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{} {} {}] {}",
                humantime::format_rfc3339_seconds(SystemTime::now()),
                colors.color(record.level()),
                record.target(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .level_for("tracing", log::LevelFilter::Warn) // This spams heartbeats
        .level_for("serenity", log::LevelFilter::Warn)
        .level_for("h2", log::LevelFilter::Warn)
        .level_for("rustls", log::LevelFilter::Warn)
        .level_for("hyper", log::LevelFilter::Warn)
        .level_for("reqwest", log::LevelFilter::Warn)
        .level_for("tungstenite", log::LevelFilter::Warn)
        .chain(std::io::stderr())
        // .chain(std::io::stdout())
        // .chain(fern::log_file("output.log")?)
        .apply()?;

    // Configure the client with your Discord bot token in the environment.
    let token = std::env::var("DISCORD_TOKEN").expect("Expected DISCORD_TOKEN in the environment");
    serenity::utils::token::validate(token.clone()).unwrap();

    // Initiate a connection to the database file, creating the file if required.
    let database = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename("prod.db")
                .create_if_missing(true),
        )
        .await
        .expect("Couldn't connect to database");

    // Run migrations, which updates the database's schema to the latest version.
    sqlx::migrate!("./migrations")
        .run(&database)
        .await
        .expect("Couldn't run database migrations");

    let bot = Bot { database };

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;
    let mut client = Client::builder(&token, intents)
        .event_handler(bot)
        .await
        .expect("Err creating client");

    if let Err(err) = client.start().await {
        error!("Client start failed: {err:?}");
        return Err(err)?;
    }

    Ok(())
}
