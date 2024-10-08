CREATE TABLE chats
(
    id BIGINT PRIMARY KEY,
    name VARCHAR NOT NULL
);

CREATE TABLE topics
(
    message_thread_id INTEGER PRIMARY KEY,
    chat_id BIGINT NOT NULL REFERENCES chats(id) ON DELETE CASCADE,
    play_id INTEGER NOT NULL REFERENCES plays(id) ON DELETE CASCADE,
    last_updated TIMESTAMP WITH TIME ZONE NOT NULL
);
