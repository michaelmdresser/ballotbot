CREATE TABLE session_participants (
session_id INTEGER NOT NULL,
participant TEXT NOT NULL,

FOREIGN KEY(session_id) REFERENCES voting_session(id),
PRIMARY KEY(session_id, participant)
)
