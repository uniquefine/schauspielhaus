--- add i64 column for pinned message hash, by default set it to 0
ALTER TABLE topics ADD COLUMN pinned_message_hash BIGINT NOT NULL DEFAULT 0;
