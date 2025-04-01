CREATE TABLE session_ballot_options (
session_id INTEGER NOT NULL,
suggester TEXT NOT NULL,
option_name TEXT NOT NULL,
option_id INTEGER NOT NULL,

FOREIGN KEY(session_id) REFERENCES voting_session(id),
UNIQUE(session_id, suggester),
UNIQUE(session_id, suggester, option_id),
PRIMARY KEY(session_id, option_id)
)
