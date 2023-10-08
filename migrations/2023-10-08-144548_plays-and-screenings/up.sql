CREATE TABLE plays
(
    id SERIAL PRIMARY KEY,
    url VARCHAR NOT NULL UNIQUE,
    name VARCHAR NOT NULL,
    description VARCHAR NOT NULL,
    image_url VARCHAR NOT NULL,
    meta_info VARCHAR NOT NULL
);

CREATE TABLE screenings
(
    id SERIAL PRIMARY KEY,
    play_id INTEGER NOT NULL REFERENCES plays(id) ON DELETE CASCADE,
    webid VARCHAR NOT NULL UNIQUE,
    location VARCHAR NOT NULL,
    url VARCHAR NOT NULL UNIQUE,
    start_time TIMESTAMP WITH TIME ZONE NOT NULL
);

