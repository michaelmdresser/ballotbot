//! Bounded, aggregate-only Condorcet / winning-votes Schulze elections.
//!
//! Candidate identity is independent of labels and input ordering. Ballots must
//! be complete permutations of the declared IDs. Zero candidates/ballots are
//! errors; a single candidate with at least one valid ballot wins.
#![forbid(unsafe_code)]

use nalgebra::DMatrix;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

pub const MAX_CANDIDATES: usize = 128;
pub const MAX_BALLOTS: usize = 100_000;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CandidateId(pub u64);

pub type Ballot = Vec<CandidateId>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Method {
    Condorcet,
    SchulzeWinningVotes,
}

/// Contains no individual rankings or per-ballot matrices.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Aggregate {
    pub candidates: Vec<CandidateId>,
    pub winners: Vec<CandidateId>,
    pub method: Method,
    pub ballot_count: u32,
    pub pairwise: DMatrix<u32>,
    pub strongest_paths: Option<DMatrix<u32>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VoteError {
    CandidateCount,
    BallotCount,
    DuplicateCandidate,
    InvalidBallot { ballot: usize },
    Capacity,
    Invariant,
}

impl fmt::Display for VoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CandidateCount => write!(f, "candidate count must be 1..={MAX_CANDIDATES}"),
            Self::BallotCount => write!(f, "ballot count must be 1..={MAX_BALLOTS}"),
            Self::DuplicateCandidate => f.write_str("candidate IDs must be unique"),
            Self::InvalidBallot { ballot } => {
                write!(f, "ballot {ballot} is not a complete permutation")
            }
            Self::Capacity => f.write_str("election allocation/count capacity exceeded"),
            Self::Invariant => f.write_str("election invariant failed"),
        }
    }
}

impl std::error::Error for VoteError {}

fn cell(matrix: &DMatrix<u32>, row: usize, col: usize) -> Result<u32, VoteError> {
    matrix.get((row, col)).copied().ok_or(VoteError::Invariant)
}

fn set(matrix: &mut DMatrix<u32>, row: usize, col: usize, value: u32) -> Result<(), VoteError> {
    *matrix.get_mut((row, col)).ok_or(VoteError::Invariant)? = value;
    Ok(())
}

fn zero_matrix(n: usize) -> Result<DMatrix<u32>, VoteError> {
    let len = n.checked_mul(n).ok_or(VoteError::Capacity)?;
    let mut cells = Vec::new();
    cells
        .try_reserve_exact(len)
        .map_err(|_| VoteError::Capacity)?;
    cells.resize(len, 0);
    Ok(DMatrix::from_vec(n, n, cells))
}

pub fn vote(candidates: &[CandidateId], ballots: &[Ballot]) -> Result<Aggregate, VoteError> {
    let n = candidates.len();
    if n == 0 || n > MAX_CANDIDATES {
        return Err(VoteError::CandidateCount);
    }
    if ballots.is_empty() || ballots.len() > MAX_BALLOTS {
        return Err(VoteError::BallotCount);
    }
    let ballot_count = u32::try_from(ballots.len()).map_err(|_| VoteError::Capacity)?;
    let indices: BTreeMap<_, _> = candidates.iter().copied().zip(0..n).collect();
    if indices.len() != n {
        return Err(VoteError::DuplicateCandidate);
    }
    let mut pairwise = zero_matrix(n)?;
    for (ballot_index, ballot) in ballots.iter().enumerate() {
        let invalid = || VoteError::InvalidBallot {
            ballot: ballot_index,
        };
        if ballot.len() != n {
            return Err(invalid());
        }
        let mut seen = BTreeSet::new();
        let mut positions = vec![0; n];
        for (position, id) in ballot.iter().enumerate() {
            let index = indices.get(id).copied().ok_or_else(invalid)?;
            if !seen.insert(*id) {
                return Err(invalid());
            }
            *positions.get_mut(index).ok_or(VoteError::Invariant)? = position;
        }
        for (runner, rank) in positions.iter().enumerate() {
            for (opponent, other_rank) in positions.iter().enumerate() {
                if rank < other_rank {
                    let count = cell(&pairwise, runner, opponent)?;
                    set(
                        &mut pairwise,
                        runner,
                        opponent,
                        count.checked_add(1).ok_or(VoteError::Capacity)?,
                    )?;
                }
            }
        }
    }
    // Always-on validation of the complete-permutation pairwise invariant.
    for runner in 0..n {
        for opponent in 0..n {
            let forward = cell(&pairwise, runner, opponent)?;
            let reverse = cell(&pairwise, opponent, runner)?;
            if (runner == opponent && forward != 0)
                || (runner != opponent && forward.checked_add(reverse) != Some(ballot_count))
            {
                return Err(VoteError::Invariant);
            }
        }
    }
    let (method, winners, strongest_paths) = outcome(candidates, &pairwise)?;
    Ok(Aggregate {
        candidates: candidates.to_vec(),
        winners,
        method,
        ballot_count,
        pairwise,
        strongest_paths,
    })
}

type Outcome = (Method, Vec<CandidateId>, Option<DMatrix<u32>>);

// Extracted from ballotbot's condorcet_voting.rs: strict pairwise Condorcet
// winner, then winning-votes initialization and max/min Floyd-Warshall paths.
// Unlike the old score sort, retain every unbeaten Schulze candidate as a tie.
fn outcome(candidates: &[CandidateId], pairwise: &DMatrix<u32>) -> Result<Outcome, VoteError> {
    let n = candidates.len();
    for (runner, id) in candidates.iter().enumerate() {
        let mut wins = true;
        for opponent in 0..n {
            if runner != opponent
                && cell(pairwise, runner, opponent)? <= cell(pairwise, opponent, runner)?
            {
                wins = false;
            }
        }
        if wins {
            return Ok((Method::Condorcet, vec![*id], None));
        }
    }
    let mut paths = zero_matrix(n)?;
    for runner in 0..n {
        for opponent in 0..n {
            let forward = cell(pairwise, runner, opponent)?;
            if forward > cell(pairwise, opponent, runner)? {
                set(&mut paths, runner, opponent, forward)?;
            }
        }
    }
    for via in 0..n {
        for runner in 0..n {
            if runner == via {
                continue;
            }
            for opponent in 0..n {
                if opponent == via || runner == opponent {
                    continue;
                }
                let strength = cell(&paths, runner, via)?.min(cell(&paths, via, opponent)?);
                let best = cell(&paths, runner, opponent)?.max(strength);
                set(&mut paths, runner, opponent, best)?;
            }
        }
    }
    let mut winners = Vec::new();
    for (runner, id) in candidates.iter().enumerate() {
        let mut unbeaten = true;
        for opponent in 0..n {
            if cell(&paths, runner, opponent)? < cell(&paths, opponent, runner)? {
                unbeaten = false;
            }
        }
        if unbeaten {
            winners.push(*id);
        }
    }
    if winners.is_empty() {
        return Err(VoteError::Invariant);
    }
    Ok((Method::SchulzeWinningVotes, winners, Some(paths)))
}
