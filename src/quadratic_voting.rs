pub mod qv {
    use std::collections::BTreeMap;
    use std::convert::TryInto;
    use std::error::Error;
    use std::fmt;

    pub type BallotChoice = i64;
    pub type Votes = i64;
    pub type Ballot = BTreeMap<BallotChoice, Votes>;

    pub fn tokens_used(b: &Ballot) -> i64 {
        // Abs is so we can support negative votes
        // TODO: is "as" okay here?
        b.iter().fold(0, |acc, (_, votes)| acc + votes.abs().pow(2))
    }

    #[test]
    fn testtokensused() {
        struct Case {
            input: Ballot,
            expected: i64,
        }

        let cases = [
            Case {
                input: BTreeMap::from([(0, 10), (1, 2)]),
                expected: 104,
            },
            Case {
                input: BTreeMap::from([(0, 10)]),
                expected: 100,
            },
            Case {
                input: BTreeMap::from([]),
                expected: 0,
            },
            Case {
                input: BTreeMap::from([(0, 10), (1, -2)]),
                expected: 104,
            },
        ];

        for case in cases.iter() {
            let tokens = tokens_used(&case.input);
            assert_eq!(tokens, case.expected);
        }
    }

    pub fn valid_ballot(b: &Ballot, max_tokens: i64, num_choices: i64) -> bool {
        if tokens_used(b) > max_tokens {
            return false;
        }

        for (choice, _) in b.iter() {
            if choice > &(num_choices - 1) {
                return false;
            }
        }

        return true;
    }

    fn winner(b: &Ballot) -> BallotChoice {
        b.iter()
            .fold((0, 0), |(winner, winner_votes), (choice, choice_votes)| {
                // TODO: How to handle tie?
                if choice_votes > &winner_votes {
                    (*choice, *choice_votes)
                } else {
                    (winner, winner_votes)
                }
            })
            .0
    }

    #[test]
    fn testvalidballot() {
        struct Case {
            b: Ballot,
            max_tokens: i64,
            num_choices: i64,
            expected: bool,
        }

        let cases = [
            Case {
                b: BTreeMap::from([(0, 10), (1, 2)]),
                max_tokens: 100,
                num_choices: 2,
                expected: false,
            },
            Case {
                b: BTreeMap::from([(0, 10), (1, 2)]),
                max_tokens: 104,
                num_choices: 1,
                expected: false,
            },
            Case {
                b: BTreeMap::from([]),
                max_tokens: 104,
                num_choices: 1,
                expected: true,
            },
            Case {
                b: BTreeMap::from([(0, 10), (1, 1)]),
                max_tokens: 104,
                num_choices: 3,
                expected: true,
            },
        ];

        for case in cases.iter() {
            assert_eq!(
                valid_ballot(&case.b, case.max_tokens, case.num_choices),
                case.expected
            );
        }
    }

    #[test]
    fn testwinner() {
        struct Case {
            b: Ballot,
            expected: BallotChoice,
        }

        let cases = [
            Case {
                b: BTreeMap::from([(0, 10), (1, 2)]),
                expected: 0,
            },
            Case {
                b: BTreeMap::from([(0, 10), (1, 11)]),
                expected: 1,
            },
            Case {
                b: BTreeMap::from([(3, 1)]),
                expected: 3,
            },
            // TODO: Tie?
            // TODO: Empty ballot?
        ];

        for case in cases.iter() {
            assert_eq!(winner(&case.b), case.expected);
        }
    }

    fn aggregate_ballots(ballots: Vec<&Ballot>) -> Ballot {
        ballots
            .into_iter()
            .fold(Ballot::new(), |mut result, ballot| {
                for (choice, ballot_votes) in ballot.iter() {
                    *result.entry(*choice).or_insert(0) += ballot_votes;
                }
                return result;
            })
    }

    #[test]
    fn testaggregate() {
        struct Case {
            ballots: Vec<Ballot>,
            expected: Ballot,
        }

        let cases = [
            Case {
                ballots: Vec::from([BTreeMap::from([(0, 10), (1, 2)])]),
                expected: BTreeMap::from([(0, 10), (1, 2)]),
            },
            Case {
                ballots: Vec::from([BTreeMap::from([(1, 2)])]),
                expected: BTreeMap::from([(1, 2)]),
            },
            Case {
                ballots: Vec::from([BTreeMap::from([])]),
                expected: BTreeMap::from([]),
            },
            Case {
                ballots: Vec::from([
                    BTreeMap::from([(1, 2)]),
                    BTreeMap::from([(0, 11)]),
                    BTreeMap::from([(3, 4)]),
                    BTreeMap::from([(1, 3)]),
                ]),
                expected: BTreeMap::from([(1, 5), (0, 11), (3, 4)]),
            },
            Case {
                ballots: Vec::from([
                    BTreeMap::from([(0, 2), (1, 4), (2, 8)]),
                    BTreeMap::from([(0, 11), (1, 1), (2, 3)]),
                    BTreeMap::from([(0, 0), (1, 13), (3, 7)]),
                ]),
                expected: BTreeMap::from([(0, 13), (1, 18), (2, 11), (3, 7)]),
            },
        ];

        for case in cases.iter() {
            assert_eq!(
                aggregate_ballots(case.ballots.iter().collect()),
                case.expected
            );
        }
    }

    #[derive(Debug, PartialEq)]
    pub struct VoteReport {
        pub num_voters: i64,
        pub total_tokens_available: i64,
        pub total_tokens_remaining: i64,
        pub votes: Ballot,
        pub winner: BallotChoice,
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

    pub fn vote(ballots: Vec<&Ballot>, tokens_per_ballot: i64) -> Result<VoteReport, VoteError> {
        for ballot in ballots.iter() {
            if !valid_ballot(ballot, tokens_per_ballot, std::i64::MAX) {
                // TODO: Better error
                return Err(VoteError::new(&format!("invalid ballot: {:#?}", ballot)));
            }
        }
        // TODO: Is there a way to avoid this clone?
        let final_ballot = aggregate_ballots(ballots.clone());
        let tokens_spent = tokens_used(&final_ballot);

        let num_voters: i64 = ballots.len().try_into().unwrap();
        // .map_err(|err| VoteError::new(format!("getting number of ballots: {}", err)))?;
        let total_tokens_available = num_voters * tokens_per_ballot;
        let total_tokens_remaining = total_tokens_available - tokens_spent;
        return Ok(VoteReport {
            num_voters,
            total_tokens_available,
            total_tokens_remaining,
            winner: winner(&final_ballot),
            votes: final_ballot,
        });
    }

    #[test]
    fn testvote() {
        struct Case {
            ballots: Vec<Ballot>,
            expected_winner: BallotChoice,
        }

        let cases = [
            Case {
                ballots: Vec::from([BTreeMap::from([(0, 10), (1, 2)])]),
                expected_winner: 0,
            },
            Case {
                ballots: Vec::from([BTreeMap::from([(1, 2)])]),
                expected_winner: 1,
            },
            Case {
                // TODO: this behavior is kind of undefined
                ballots: Vec::from([BTreeMap::from([])]),
                expected_winner: 0,
            },
            Case {
                ballots: Vec::from([
                    BTreeMap::from([(1, 2)]),
                    BTreeMap::from([(0, 4)]),
                    BTreeMap::from([(3, 4)]),
                    BTreeMap::from([(1, 3)]),
                ]),
                expected_winner: 1,
            },
            Case {
                ballots: Vec::from([
                    BTreeMap::from([(0, 2), (1, 4), (2, 8)]),
                    BTreeMap::from([(0, 11), (1, 1), (2, 3)]),
                    BTreeMap::from([(0, 0), (1, 13), (3, 7)]),
                ]),
                expected_winner: 1,
            },
        ];

        for case in cases.iter() {
            match vote(case.ballots.iter().collect(), 1004) {
                Ok(result) => assert_eq!(result.winner, case.expected_winner),
                Err(err) => assert!(false, "{}", err),
            };
        }
    }
}

// fn parse_qv_ballot(ballot_str: String) -> Result<qv::Ballot, ParseError> {
//     ballot_str
//         .trim()
//         .split(",")
//         .try_fold(BTreeMap::new(), |mut map, vote_raw| {
//             let split = vote_raw.split(":").collect::<Vec<&str>>();
//             if split.len() != 2 {
//                 return Err(ParseError::new(&format!(
//                     "Vote '{vote_raw}' is an invalid format"
//                 )));
//             }
//             let ballot_key = match split[0].trim().parse::<i64>() {
//                 Ok(x) => x,
//                 Err(err) => {
//                     return Err(ParseError::new(&format!(
//                         "Failed to parse first part of '{vote_raw}' as an i64: {err}"
//                     )))
//                 }
//             };
//             let ballot_votes = match split[1].trim().parse::<i64>() {
//                 Ok(x) => x,
//                 Err(err) => {
//                     return Err(ParseError::new(&format!(
//                         "Failed to parse second part of '{vote_raw}' as an i64: {err}"
//                     )))
//                 }
//             };

//             if map.contains_key(&ballot_key) {
//                 return Err(ParseError::new(&format!(
//                     "Invalid vote '{vote_raw}'; cannot vote twice for the same candidate"
//                 )));
//             }

//             map.insert(ballot_key, ballot_votes);
//             Ok(map)
//         })
// }

// #[test]
// fn test_parse_qv_ballot() {
//     struct Case {
//         input: String,
//         expected: qv::Ballot,
//     }

//     let cases = [Case {
//         input: "1: 3, 4: 6, 9: 100".to_string(),
//         expected: qv::Ballot::from([(1, 3), (4, 6), (9, 100)]),
//     }];

//     for case in cases.iter() {
//         assert_eq!(parse_ballot(case.input.clone()).unwrap(), case.expected,);
//     }
// }
