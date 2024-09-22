// @generated automatically by Diesel CLI.

diesel::table! {
    chats (id) {
        id -> Int8,
        name -> Varchar,
    }
}

diesel::table! {
    plays (id) {
        id -> Int4,
        url -> Varchar,
        name -> Varchar,
        description -> Varchar,
        image_url -> Varchar,
        meta_info -> Varchar,
    }
}

diesel::table! {
    screenings (id) {
        id -> Int4,
        play_id -> Int4,
        webid -> Varchar,
        location -> Varchar,
        url -> Varchar,
        start_time -> Timestamptz,
    }
}

diesel::table! {
    topics (message_thread_id) {
        message_thread_id -> Int4,
        chat_id -> Int8,
        play_id -> Int4,
        last_updated -> Timestamptz,
        pinned_message_id -> Int4,
    }
}

diesel::joinable!(screenings -> plays (play_id));
diesel::joinable!(topics -> chats (chat_id));
diesel::joinable!(topics -> plays (play_id));

diesel::allow_tables_to_appear_in_same_query!(
    chats,
    plays,
    screenings,
    topics,
);
