pub mod rcv {
    pub type BallotChoice = i64;
    pub type Ballot = Vec<BallotChoice>;

    pub struct VoteBreakdown {
        s: String,
    }

    // TODO: Better error...
    #[derive(Debug)]
    pub struct VoteError {
        details: String,
    }

    impl VoteError {
        fn new(msg: &str) -> VoteError {
            VoteError {
                details: msg.to_string(),
            }
        }
    }

    impl fmt::Display for VoteError {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{}", self.details)
        }
    }

    impl Error for VoteError {
        fn description(&self) -> &str {
            &self.details
        }
    }

    #[test]
    fn test_round_winner() {
        struct Case {
            ballots: Vec<Ballot>,
            expected: Option<BallotChoice>,
        }

        let cases = [
            Case {
                ballots: Vec::from([
                    Ballot::from([3, 1, 2]),
                    Ballot::from([1, 3, 2]),
                    Ballot::from([3, 2, 1]),
                ]),
                expected: Some(3),
            },
            Case {
                ballots: Vec::from([
                    Ballot::from([2, 1, 3]),
                    Ballot::from([1, 3, 2]),
                    Ballot::from([3, 2, 1]),
                ]),
                expected: None,
            },
            Case {
                ballots: Vec::from([
                    Ballot::from([2, 1, 3]),
                    Ballot::from([2, 3, 1]),
                    Ballot::from([3, 1, 2]),
                    Ballot::from([3, 2, 1]),
                ]),
                expected: None,
            },
            Case {
                ballots: Vec::from([Ballot::from([2, 1, 3])]),
                expected: Some(2),
            },
            Case {
                ballots: Vec::from([]),
                expected: None,
            },
        ];

        for case in cases.iter() {
            assert_eq!(round_winner(case.ballots, case.round), case.expected,)
        }
    }

    pub struct RoundBreakdown {
        starting_ballots: Vec<Ballot>,
        votes_by_candidate: BTreeMap<BallotChoice, i64>,
        eliminated: Option<BallotChoice>,
        ending_ballots: Vec<Ballot>,
        winner: Option<BallotChoice>,
    }

    fn eliminate_option(ballots: Vec<Ballot>, eliminated: BallotChoice) -> Vec<Ballot> {
        ballots
            .iter()
            .map(|ballot| {
                ballot
                    .into_iter()
                    .filter(|choice| choice != eliminated)
                    .collect()
            })
            .collect()
    }

    #[test]
    fn test_pick_eliminate() {
        struct Case {
            ballots: Vec<Ballot>,
            expected: BallotChoice,
        }

        let cases = [
            Case {
                ballots: Vec::from([
                    Ballot::from([3, 1, 2]),
                    Ballot::from([1, 3, 2]),
                    Ballot::from([3, 2, 1]),
                ]),
                expected: 2,
            },
            Case {
                ballots: Vec::from([
                    Ballot::from([3, 1, 2, 4]),
                    Ballot::from([2, 3, 1, 4]),
                    Ballot::from([3, 1, 2, 4]),
                    Ballot::from([2, 1, 3, 4]),
                ]),
                expected: 2,
            },
        ];

        for case in cases.iter() {
            assert_eq!(pick_eliminate(case.ballots), expected);
        }
    }

    fn pick_eliminate(ballots: Vec<Ballot>) -> BallotChoice {}

    fn run_round(ballots: Vec<Ballot>) -> RoundBreakdown {
        let winner_requirement = (ballots.len() / 2) + 1;

        let mut votes_by_candidate = BTreeMap::new();
        for ballot in ballots.iter() {
            if let Some(choice) = ballot.get(0) {
                match votes_by_candidate.get(choice) {
                    Some(votes) => {
                        votes_by_candidate.insert(choice, votes + 1);
                    }
                    None => {
                        votes_by_candidate.insert(choice, 1);
                    }
                }
            }
        }

        for (choice, votes) in &votes_by_candidate {
            if votes >= winner_requirement {
                return RoundBreakdown {
                    starting_ballots: ballots,
                    votes_by_candidate,
                    eliminated: None,
                    ending_ballots: ballots,
                    winner: Some(choice),
                };
            }
        }

        let eliminated = pick_eliminate(ballots);
        let ending_ballots = eliminate_option(ballots, eliminated);

        return RoundBreakdown {
            starting_ballots: ballots,
            votes_by_candidate,
            eliminated,
            ending_ballots,
            winner: None,
        };
    }

    pub fn vote(
        num_choices: i64,
        ballots: Vec<Ballot>,
    ) -> Result<Tuple<BallotChoice, VoteBreakdown>, VoteError> {
        for (i, ballot) in ballots.iter().enumerate() {
            if ballot.len() != num_choices {
                return VoteError::new(format!(
                    "Ballot {i} ({}) has an invalid number of choices",
                    ballot
                ));
            }
        }
    }
}
