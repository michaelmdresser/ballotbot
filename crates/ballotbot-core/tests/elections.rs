#![allow(clippy::expect_used, clippy::indexing_slicing)]

use ballotbot_core::{Ballot, CandidateId, Method, VoteError, vote};

fn ids(values: &[u64]) -> Vec<CandidateId> {
    values.iter().copied().map(CandidateId).collect()
}

fn ballots(values: &[&[u64]]) -> Vec<Ballot> {
    values.iter().map(|values| ids(values)).collect()
}

#[test]
fn original_condorcet_fixture_and_pairwise_invariants() {
    let candidates = ids(&[0, 1, 2, 3]);
    let ballots = ballots(&[&[1, 2, 0, 3], &[3, 0, 2, 1], &[0, 2, 1, 3]]);
    let result = vote(&candidates, &ballots).expect("valid");
    assert_eq!(result.winners, ids(&[0]));
    assert_eq!(result.method, Method::Condorcet);
    assert_eq!(
        result.pairwise,
        nalgebra::DMatrix::from_row_slice(4, 4, &[0, 2, 2, 2, 1, 0, 1, 2, 1, 2, 0, 2, 1, 1, 1, 0,])
    );
    for a in 0..4 {
        for b in 0..4 {
            if a != b {
                assert_eq!(result.pairwise[(a, b)] + result.pairwise[(b, a)], 3);
            }
        }
    }
    let mut reversed = ballots.clone();
    reversed.reverse();
    assert_eq!(vote(&candidates, &reversed).expect("valid"), result);
}

#[test]
fn canonical_winning_votes_schulze() {
    // Wikipedia's 45-voter example; E wins, with no Condorcet winner.
    let groups: &[(usize, &[u64])] = &[
        (5, &[0, 2, 1, 4, 3]),
        (5, &[0, 3, 4, 2, 1]),
        (8, &[1, 4, 3, 0, 2]),
        (3, &[2, 0, 1, 4, 3]),
        (7, &[2, 0, 4, 1, 3]),
        (2, &[2, 1, 0, 3, 4]),
        (7, &[3, 2, 4, 1, 0]),
        (8, &[4, 1, 0, 3, 2]),
    ];
    let ballots: Vec<_> = groups
        .iter()
        .flat_map(|(count, ranking)| std::iter::repeat_n(ids(ranking), *count))
        .collect();
    let result = vote(&ids(&[0, 1, 2, 3, 4]), &ballots).expect("valid");
    assert_eq!(result.method, Method::SchulzeWinningVotes);
    assert_eq!(result.winners, ids(&[4]));
    assert_eq!(
        result.strongest_paths.expect("Schulze paths"),
        nalgebra::DMatrix::from_row_slice(
            5,
            5,
            &[
                0, 28, 28, 30, 24, 25, 0, 28, 33, 24, 25, 29, 0, 29, 24, 25, 28, 28, 0, 24, 25, 28,
                28, 31, 0,
            ]
        )
    );
}

#[test]
fn exact_tie_and_symmetric_cycle_do_not_break_ties_by_order() {
    let result = vote(&ids(&[91, 4]), &ballots(&[&[91, 4], &[4, 91]])).expect("tie");
    assert_eq!(result.winners, ids(&[91, 4]));
    let cycle = ballots(&[&[0, 1, 2], &[1, 2, 0], &[2, 0, 1]]);
    assert_eq!(
        vote(&ids(&[0, 1, 2]), &cycle).expect("cycle").winners,
        ids(&[0, 1, 2])
    );
    assert_eq!(
        vote(&ids(&[2, 0, 1]), &cycle).expect("reorder").winners,
        ids(&[2, 0, 1])
    );
}

#[test]
fn rejects_malformed_and_defines_degenerate_elections() {
    for ballot in [&[1, 1][..], &[1, 9], &[1], &[1, 2, 3]] {
        assert!(matches!(
            vote(&ids(&[1, 2]), &[ids(ballot)]),
            Err(VoteError::InvalidBallot { .. })
        ));
    }
    assert_eq!(vote(&[], &[]), Err(VoteError::CandidateCount));
    assert_eq!(vote(&ids(&[1]), &[]), Err(VoteError::BallotCount));
    assert_eq!(
        vote(&ids(&[1, 1]), &ballots(&[&[1, 1]])),
        Err(VoteError::DuplicateCandidate)
    );
    assert_eq!(
        vote(&ids(&[700]), &ballots(&[&[700]]))
            .expect("one")
            .winners,
        ids(&[700])
    );
    assert_eq!(
        vote(&vec![CandidateId(0); 129], &[]),
        Err(VoteError::CandidateCount)
    );
}

#[test]
fn relabeling_preserves_winners_and_counts() {
    for offset in [0, 100, u64::MAX - 3] {
        let candidates = ids(&[offset, offset + 1, offset + 2]);
        let result = vote(&candidates, &[candidates.clone(), candidates.clone()]).expect("valid");
        assert_eq!(result.winners, ids(&[offset]));
        assert_eq!(result.pairwise[(0, 1)], 2);
    }
}
