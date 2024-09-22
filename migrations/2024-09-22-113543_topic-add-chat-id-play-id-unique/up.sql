--- Add a unique contraint for topic message_thread_id and chat_id
ALTER TABLE topics ADD CONSTRAINT topics_message_thread_id_chat_id_unique UNIQUE (message_thread_id, chat_id);
