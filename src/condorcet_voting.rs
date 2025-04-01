pub mod cv {
    use nalgebra;
    use std::convert::TryFrom;
    use std::error::Error;
    use std::fmt;

    pub type BallotChoice = u32;
    pub type Ballot = Vec<BallotChoice>;

    #[derive(Debug, Clone)]
    pub enum WinType {
        CondorcetWinner,
        SchulzeRanking,
    }

    #[derive(Debug, Clone)]
    pub struct VoteBreakdown {
        pub winner: Option<BallotChoice>,
        pub win_type: Option<WinType>,
        pub ballots: Vec<Ballot>,
        pub ballot_outranking: Vec<nalgebra::DMatrix<u32>>,
        pub final_outranking: nalgebra::DMatrix<u32>,
        pub schulze_result: Option<(Vec<(BallotChoice, u32)>, nalgebra::DMatrix<u32>)>,
    }

    impl std::fmt::Display for VoteBreakdown {
        fn fmt(&self, fmt: &mut fmt::Formatter) -> std::fmt::Result {
            match (self.winner, self.win_type.clone()) {
                (Some(winner), Some(win_type)) => {
                    fmt.write_str(&format!("Winner - {winner}. Win Type: {:?}", win_type))?;
                }
                (None, _) => {
                    fmt.write_str("No winner!")?;
                }
                (_, _) => panic!("huh?!"), // FIXME
            };
            fmt.write_str("\n")?;
            fmt.write_str("Ballots:")?;
            for ballot in &self.ballots {
                fmt.write_str(&format!("  {:?}\n", ballot))?;
            }
            fmt.write_str("\n")?;
            fmt.write_str("Final outranking matrix")?;
            fmt.write_str(&format!("  {}", self.final_outranking))?;

            if let Some(schulze_result) = &self.schulze_result {
                fmt.write_str("\n")?;
                fmt.write_str(&format!("  Schulze Ranking: {:?}", schulze_result.0))?;
                fmt.write_str("  Schulze path strength matrix:")?;
                fmt.write_str(&format!("    {}", schulze_result.1))?;
            }

            Ok(())
        }
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
    fn howdoesindexingwork() {
        let m = nalgebra::DMatrix::from_rows(&[
            nalgebra::dvector![0, 0, 0, 1].transpose(), //
            nalgebra::dvector![1, 0, 1, 1].transpose(), //
            nalgebra::dvector![1, 3, 0, 1].transpose(), //
            nalgebra::dvector![0, 0, 0, 0].transpose(), //
        ]);

        assert_eq!(m[(2, 1)], 3);
    }

    #[test]
    fn test_ballot_to_outranking_matrix() {
        struct Case {
            ballot: Ballot,
            expected: nalgebra::DMatrix<u32>,
        }

        let cases = [
            Case {
                // B, C, A, D
                ballot: Vec::from([1, 2, 0, 3]),
                expected: nalgebra::DMatrix::from_rows(&[
                    nalgebra::dvector![0, 0, 0, 1].transpose(), //
                    nalgebra::dvector![1, 0, 1, 1].transpose(), //
                    nalgebra::dvector![1, 0, 0, 1].transpose(), //
                    nalgebra::dvector![0, 0, 0, 0].transpose(), //
                ]),
            },
            Case {
                // C, B, A, D
                ballot: Vec::from([2, 1, 0, 3]),
                expected: nalgebra::DMatrix::from_rows(&[
                    nalgebra::dvector![0, 0, 0, 1].transpose(), //
                    nalgebra::dvector![1, 0, 0, 1].transpose(), //
                    nalgebra::dvector![1, 1, 0, 1].transpose(), //
                    nalgebra::dvector![0, 0, 0, 0].transpose(), //
                ]),
            },
        ];

        for (i, case) in cases.iter().enumerate() {
            assert_eq!(
                ballot_to_outranking_matrix(&case.ballot),
                case.expected,
                "Case {}",
                i,
            );
        }
    }

    //        https://en.wikipedia.org/wiki/Condorcet_method
    fn ballot_to_outranking_matrix(ballot: &Ballot) -> nalgebra::DMatrix<u32> {
        let mut candidate_to_ballot_position: Vec<usize> = vec![ballot.len() + 10; ballot.len()];
        for (rank, candidate) in ballot.iter().enumerate() {
            candidate_to_ballot_position[usize::try_from(*candidate).unwrap()] = rank;
        }

        for rank in candidate_to_ballot_position.iter() {
            if *rank >= ballot.len().try_into().unwrap() {
                panic!("Impossible rank {rank}")
            }
        }

        return nalgebra::DMatrix::from_fn(ballot.len(), ballot.len(), |runner, opponent| {
            if runner == opponent {
                return 0;
            }

            if candidate_to_ballot_position[runner] < candidate_to_ballot_position[opponent] {
                return 1;
            } else {
                return 0;
            }
        });
    }

    #[test]
    fn test_condorcet_winner() {
        struct Case {
            m: nalgebra::DMatrix<u32>,
            expected: Option<BallotChoice>,
        }

        let cases = [
            Case {
                m: nalgebra::DMatrix::from_row_slice(
                    4,
                    4,
                    &[
                        0, 2, 2, 2, //
                        1, 0, 1, 2, //
                        1, 2, 0, 2, //
                        1, 1, 1, 0,
                    ],
                ),
                expected: Some(0),
            },
            Case {
                m: nalgebra::DMatrix::from_row_slice(
                    5,
                    5,
                    // https://en.wikipedia.org/wiki/Schulze_method
                    &[
                        0, 20, 26, 30, 22, //
                        25, 0, 16, 33, 18, //
                        19, 29, 0, 17, 24, //
                        15, 12, 28, 0, 14, //
                        23, 27, 21, 31, 0, //
                    ],
                ),
                expected: None,
            },
        ];

        for case in cases.iter() {
            assert_eq!(condorcet_winner(&case.m), case.expected);
        }
    }

    fn condorcet_winner(overall_matrix: &nalgebra::DMatrix<u32>) -> Option<BallotChoice> {
        let num_candidates = overall_matrix.row(0).len();
        for runner in 0..num_candidates {
            let mut runner_failed = false;
            for opponent in 0..num_candidates {
                if runner == opponent {
                    continue;
                }

                if overall_matrix[(runner, opponent)] <= overall_matrix[(opponent, runner)] {
                    runner_failed = true;
                    break;
                }
            }

            if !runner_failed {
                return Some(runner.try_into().unwrap());
            }
        }
        return None;
    }

    #[test]
    fn test_schulze_ranking() {
        struct Case {
            m: nalgebra::DMatrix<u32>,
            expected: Vec<(BallotChoice, u32)>,
            expected_path_matrix: nalgebra::DMatrix<u32>,
        }

        let cases = [Case {
            m: nalgebra::DMatrix::from_row_slice(
                5,
                5,
                // https://en.wikipedia.org/wiki/Schulze_method
                &[
                    0, 20, 26, 30, 22, //
                    25, 0, 16, 33, 18, //
                    19, 29, 0, 17, 24, //
                    15, 12, 28, 0, 14, //
                    23, 27, 21, 31, 0, //
                ],
            ),
            expected: vec![(4, 4), (0, 3), (2, 2), (1, 1), (3, 0)],
            expected_path_matrix: nalgebra::DMatrix::from_row_slice(
                5,
                5,
                &[
                    0, 28, 28, 30, 24, //
                    25, 0, 28, 33, 24, //
                    25, 29, 0, 29, 24, //
                    25, 28, 28, 0, 24, //
                    25, 28, 28, 31, 0, //
                ],
            ),
        }];

        for case in cases.iter() {
            let result = schulze_ranking(&case.m);
            assert_eq!(result.1, case.expected_path_matrix);
            assert_eq!(result.0, case.expected);
        }
    }

    fn schulze_path_matrix(overall_matrix: &nalgebra::DMatrix<u32>) -> nalgebra::DMatrix<u32> {
        let num_candidates = overall_matrix.row(0).len();

        // Step 1: fill with one-step path preference? is what this is?
        let mut p =
            nalgebra::DMatrix::from_fn(num_candidates, num_candidates, |runner, opponent| {
                if overall_matrix[(runner, opponent)] > overall_matrix[(opponent, runner)] {
                    return overall_matrix[(runner, opponent)];
                } else {
                    return 0;
                }
            });

        // Step 2: Do the Floyd-Warshall variant thing
        for i in 0..num_candidates {
            for j in 0..num_candidates {
                if i == j {
                    continue;
                }

                for k in 0..num_candidates {
                    if i == k || j == k {
                        continue;
                    }

                    p[(j, k)] = std::cmp::max(p[(j, k)], std::cmp::min(p[(j, i)], p[(i, k)]));
                }
            }
        }

        return p;
    }

    fn schulze_ranking(
        overall_matrix: &nalgebra::DMatrix<u32>,
    ) -> (Vec<(BallotChoice, u32)>, nalgebra::DMatrix<u32>) {
        if overall_matrix.len() == 0 {
            return (vec![], overall_matrix.clone());
        }

        let path_matrix = schulze_path_matrix(overall_matrix);

        // let mut candidate_to_num_wins: BTreeMap<BallotChoice, u32> = BTreeMap::new();
        let mut candidate_with_num_wins: Vec<(BallotChoice, u32)> = Vec::new();
        let num_candidates = overall_matrix.row(0).len();

        for runner in 0..num_candidates {
            let mut runner_wins = 0;
            for opponent in 0..num_candidates {
                if runner == opponent {
                    continue;
                }

                if path_matrix[(runner, opponent)] > path_matrix[(opponent, runner)] {
                    runner_wins += 1;
                }
            }
            candidate_with_num_wins.push((u32::try_from(runner).unwrap(), runner_wins));
        }

        candidate_with_num_wins.sort_by(|a, b| b.1.cmp(&a.1));
        return (candidate_with_num_wins, path_matrix);
    }

    #[test]
    fn test_vote() {
        struct Case {
            num_choices: u32,
            ballots: Vec<Ballot>,
            expected_winner: Option<BallotChoice>,
            expected_final_outranking: nalgebra::DMatrix<u32>,
        }

        let cases = [Case {
            num_choices: 4,
            ballots: vec![
                Ballot::from([1, 2, 0, 3]), // B, C, A, D
                Ballot::from([3, 0, 2, 1]), // D, A, C, B
                Ballot::from([0, 2, 1, 3]), // A, C, B, D
            ],
            expected_winner: Some(0),
            expected_final_outranking: nalgebra::DMatrix::from_row_slice(
                4,
                4,
                &[
                    0, 2, 2, 2, //
                    1, 0, 1, 2, //
                    1, 2, 0, 2, //
                    1, 1, 1, 0, //
                ],
            ),
        }];

        for case in cases.iter() {
            let result = vote(case.num_choices, case.ballots.clone()).unwrap();
            assert_eq!(result.final_outranking, case.expected_final_outranking);
            assert_eq!(result.winner, case.expected_winner,);
        }
    }

    pub fn vote(num_choices: u32, ballots: Vec<Ballot>) -> Result<VoteBreakdown, VoteError> {
        if ballots.len() == 0 {
            return Err(VoteError::new(&format!(
                "Must have at least one ballot to vote"
            )));
        }

        for (i, ballot) in ballots.iter().enumerate() {
            if u32::try_from(ballot.len()).unwrap() != num_choices {
                return Err(VoteError::new(&format!(
                    "Ballot {i} ({:?}) has an invalid number of choices",
                    ballot
                )));
            }
        }

        let outranking_matrices: Vec<nalgebra::DMatrix<u32>> = ballots
            .clone()
            .into_iter()
            .map(|ballot| ballot_to_outranking_matrix(&ballot))
            .collect();

        let overall_matrix = outranking_matrices
            .clone()
            .into_iter()
            .reduce(|acc, mat| acc + mat)
            .unwrap();

        if let Some(winner) = condorcet_winner(&overall_matrix) {
            return Ok(VoteBreakdown {
                winner: Some(winner),
                win_type: Some(WinType::CondorcetWinner),
                ballots,
                ballot_outranking: outranking_matrices,
                final_outranking: overall_matrix,
                schulze_result: None,
            });
        }

        let schulze_result = schulze_ranking(&overall_matrix);

        return Ok(VoteBreakdown {
            winner: Some(schulze_result.0.get(0).unwrap().0),
            win_type: Some(WinType::SchulzeRanking),
            ballots,
            ballot_outranking: outranking_matrices,
            final_outranking: overall_matrix,
            schulze_result: Some(schulze_result),
        });
    }
}
