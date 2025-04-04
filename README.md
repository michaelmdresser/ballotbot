# ballotbot

Discord bot for small-group voting. Implements the [Condorcet method](https://en.wikipedia.org/wiki/Condorcet_method) 
and the [Schulze method](https://en.wikipedia.org/wiki/Schulze_method) for voting.

Users interact by joining as voters, suggesting candidates, and voting. Joining
and suggesting occurs in a public channel; voting is by private ballot over DM
with the bot. Once all ballots are received, the bot posts the results in the
public channel.

The voting methods used by this bot are custom implementations I made for the
fun of it. They have limited testing, so I do not recommend depending upon
them. If you discover incorrect vote behavior please let me know via an
issue or email.

## Run it

Requires a Discord bot with the `GUILD_MESSAGES`, `DIRECT_MESSAGES`, and
`MESSAGE_CONTENT` intents. Put the bot token into the environment as
`DISCORD_TOKEN` and then `just run`. The bot uses a local SQLite database to
persist voting state and is thus somewhat failure-resistant. Keeping the DB
around is not necessary between runs.

## Development

Test: `just test`

## About

This bot was developed to support a small book club (4-7 active participants)
where all participants are both suggesters and voters, so there may be
some assumptions about that use-case baked in.
