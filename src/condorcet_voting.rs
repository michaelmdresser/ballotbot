//! Legacy executable adapter; algorithms live only in ballotbot-core.
pub mod cv {
    pub type Ballot = Vec<u32>;

    pub fn vote(
        candidates: &[u32],
        ballots: Vec<Ballot>,
    ) -> Result<ballotbot_core::Aggregate, ballotbot_core::VoteError> {
        if candidates.is_empty() || candidates.len() > ballotbot_core::MAX_CANDIDATES {
            return Err(ballotbot_core::VoteError::CandidateCount);
        }
        if ballots.is_empty() || ballots.len() > ballotbot_core::MAX_BALLOTS {
            return Err(ballotbot_core::VoteError::BallotCount);
        }
        if let Some((index, _)) = ballots
            .iter()
            .enumerate()
            .find(|(_, ballot)| ballot.len() != candidates.len())
        {
            return Err(ballotbot_core::VoteError::InvalidBallot { ballot: index });
        }
        let ids: Vec<_> = candidates
            .iter()
            .map(|id| ballotbot_core::CandidateId(u64::from(*id)))
            .collect();
        let ballots: Vec<_> = ballots
            .into_iter()
            .map(|ballot| {
                ballot
                    .into_iter()
                    .map(|id| ballotbot_core::CandidateId(u64::from(id)))
                    .collect()
            })
            .collect();
        ballotbot_core::vote(&ids, &ballots)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sparse_identity_and_true_ties_survive_legacy_adapter() {
        let result = super::cv::vote(&[3, 99], vec![vec![3, 99], vec![99, 3]])
            .expect("valid sparse candidates with distinct IDs even if labels match");
        assert_eq!(
            result.winners,
            vec![
                ballotbot_core::CandidateId(3),
                ballotbot_core::CandidateId(99)
            ]
        );
        assert_eq!(result.pairwise[(0, 1)], 1);
    }
}
