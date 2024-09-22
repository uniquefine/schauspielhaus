-- This file should undo anything in `up.sql`
ALTER TABLE topics DROP COLUMN pinned_message_id;
