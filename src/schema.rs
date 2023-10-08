// @generated automatically by Diesel CLI.

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

diesel::joinable!(screenings -> plays (play_id));

diesel::allow_tables_to_appear_in_same_query!(
    plays,
    screenings,
);
