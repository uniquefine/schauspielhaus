-- Your SQL goes here
--- Delete all existing topics
DELETE FROM topics;
--- Add a new column to the topic table
ALTER TABLE topics ADD COLUMN pinned_message_id INTEGER NOT NULL;
