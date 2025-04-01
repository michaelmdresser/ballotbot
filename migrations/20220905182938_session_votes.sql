CREATE TABLE session_votes (
session_id INTEGER NOT NULL,
voter TEXT NOT NULL,
ballot_option_id INTEGER NOT NULL,
rank INTEGER NOT NULL,

FOREIGN KEY(session_id) REFERENCES voting_session(id),
PRIMARY KEY(session_id, voter, ballot_option_id)
)
