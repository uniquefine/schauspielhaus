use std::collections::HashMap;

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use chrono_tz::{Europe::Zurich, Tz};
use diesel::prelude::*;
use serde;
use time::{format_description, OffsetDateTime};

#[derive(Queryable, Identifiable, Selectable, Debug, PartialEq, AsChangeset, Insertable, Clone)]
#[diesel(table_name = crate::schema::chats)]
#[diesel(check_for_backend(diesel::pg::Pg))]
#[derive(Default)]
pub struct Chat {
    pub id: i64,
    pub name: String, // just used for logging
}

#[derive(Insertable, AsChangeset, Clone)]
#[diesel(table_name = crate::schema::chats)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewChat<'a> {
    pub id: i64,
    pub name: &'a str,
}

#[derive(
    Queryable,
    Associations,
    Identifiable,
    Selectable,
    Debug,
    PartialEq,
    AsChangeset,
    Clone,
    Insertable,
)]
#[diesel(table_name = crate::schema::topics)]
#[diesel(belongs_to(Chat))]
#[diesel(belongs_to(Play))]
#[diesel(primary_key(message_thread_id))]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Topic {
    pub message_thread_id: i32,
    pub chat_id: i64,
    pub play_id: i32,
    pub last_updated: OffsetDateTime,
    pub pinned_message_id: i32,
    pub pinned_message_hash: i64,
}

#[derive(
    Default, Queryable, Identifiable, Selectable, Debug, PartialEq, serde::Serialize, Clone,
)]
#[diesel(table_name = crate::schema::plays)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Play {
    pub id: i32,
    pub url: String,
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub meta_info: String,
}

#[derive(Insertable, AsChangeset, Clone)]
#[diesel(table_name = crate::schema::plays)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewPlay<'a> {
    pub url: &'a str,
    pub name: &'a str,
    pub description: &'a str,
    pub image_url: &'a str,
    pub meta_info: &'a str,
}

#[derive(
    Queryable, Selectable, Identifiable, Associations, Debug, PartialEq, Clone, serde::Serialize,
)]
#[diesel(belongs_to(Play))]
#[diesel(table_name = crate::schema::screenings)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Screening {
    pub id: i32,
    pub play_id: i32,
    pub webid: String,
    pub location: String,
    pub url: String,
    pub start_time: OffsetDateTime,
    pub ticket_url: String,
}

pub fn to_zurich_time(offset_datetime: OffsetDateTime) -> DateTime<Tz> {
    let utc_datetime: DateTime<Utc> =
        DateTime::from_timestamp(offset_datetime.unix_timestamp(), 0).unwrap();
    let zurich_datetime = utc_datetime.with_timezone(&Zurich);
    zurich_datetime
}

// Implement std::fmt::Display for Screening
impl std::fmt::Display for Screening {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "[{}]({}{})",
            to_zurich_time(self.start_time)
                .format("%d.%m.%Y %H:%M")
                .to_string(),
            self.url,
            self.webid
        )
    }
}

#[derive(Insertable, AsChangeset, Clone)]
#[diesel(table_name = crate::schema::screenings)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct NewScreening<'a> {
    pub play_id: i32,
    pub webid: &'a str,
    pub location: &'a str,
    pub url: &'a str,
    pub start_time: OffsetDateTime,
    pub ticket_url: &'a str,
}

#[derive(Default, serde::Serialize)]
pub struct PlayWithScreenings {
    pub play: Play,
    pub screenings: Vec<Screening>,
}

pub struct NewPlayWithScreenings<'a> {
    pub play: NewPlay<'a>,
    pub screenings: Vec<NewScreening<'a>>,
}

pub struct PlayAndTopic {
    pub play: PlayWithScreenings,
    pub topic: Option<Topic>,
}

#[derive(Default)]
pub struct ChatWithTopics {
    pub chat: Chat,
    pub topics: Vec<(Topic, PlayWithScreenings)>,
}

pub fn get_chat_with_topics(
    conn: &mut PgConnection,
    chat_id: i64,
) -> Result<ChatWithTopics, diesel::result::Error> {
    use crate::schema::{chats, plays, screenings, topics};

    // Fetch the chat
    let chat = chats::table.find(chat_id).first::<Chat>(conn)?;

    // Join topics, plays, and screenings tables and filter by chat_id
    let results = topics::table
        .inner_join(plays::table.on(plays::id.eq(topics::play_id)))
        .inner_join(screenings::table.on(screenings::play_id.eq(plays::id)))
        .filter(topics::chat_id.eq(chat_id))
        .select((
            topics::all_columns,
            plays::all_columns,
            screenings::all_columns,
        ))
        .load::<(Topic, Play, Screening)>(conn)?;

    // Group the results by topic and play
    let mut topics_map: std::collections::HashMap<i32, (Topic, PlayWithScreenings)> =
        std::collections::HashMap::new();
    for (topic, play, screening) in results {
        topics_map
            .entry(topic.play_id)
            .and_modify(|(_, play_with_screenings)| {
                play_with_screenings.screenings.push(screening.clone())
            })
            .or_insert_with(|| {
                (
                    topic.clone(),
                    PlayWithScreenings {
                        play,
                        screenings: vec![screening],
                    },
                )
            });
    }

    Ok(ChatWithTopics {
        chat,
        topics: topics_map.into_iter().map(|(_, v)| v).collect(),
    })
}

pub fn get_play(
    conn: &mut PgConnection,
    play_id: i32,
) -> Result<PlayWithScreenings, diesel::result::Error> {
    use crate::schema::plays;
    use crate::schema::screenings;

    let play = plays::table.find(play_id).first::<Play>(conn)?;

    let screenings = screenings::table
        .filter(screenings::play_id.eq(play_id))
        .load::<Screening>(conn)?;

    Ok(PlayWithScreenings { play, screenings })
}

pub fn get_play_for_topic(
    conn: &mut PgConnection,
    topic_id: i32,
) -> Result<PlayWithScreenings, diesel::result::Error> {
    use crate::schema::plays;
    use crate::schema::screenings;
    use crate::schema::topics;

    let topic = topics::table.find(topic_id).first::<Topic>(conn)?;

    let play = plays::table.find(topic.play_id).first::<Play>(conn)?;

    let screenings = Screening::belonging_to(&play)
        .select(Screening::as_select())
        .order_by(screenings::start_time.asc())
        .load(conn)?;

    Ok(PlayWithScreenings { play, screenings })
}

pub fn get_screenings(
    conn: &mut PgConnection,
    play_id: i32,
) -> Result<Screening, diesel::result::Error> {
    use crate::schema::screenings;

    screenings::table.find(play_id).first::<Screening>(conn)
}

pub fn put_chat(conn: &mut PgConnection, chat: Chat) -> Result<Chat, diesel::result::Error> {
    use crate::schema::chats;
    let changeset_chat = chat.clone();
    diesel::insert_into(chats::table)
        .values(chat)
        .on_conflict(chats::id)
        .do_update()
        .set(&changeset_chat)
        .get_result::<Chat>(conn)
}

pub fn get_chat(conn: &mut PgConnection, chat_id: i64) -> Result<Chat, diesel::result::Error> {
    use crate::schema::chats;
    chats::table.find(chat_id).first::<Chat>(conn)
}

pub fn get_chats(conn: &mut PgConnection) -> Result<Vec<Chat>, diesel::result::Error> {
    use crate::schema::chats;
    chats::table.load::<Chat>(conn)
}

pub fn put_topic(conn: &mut PgConnection, topic: Topic) -> Result<Topic, diesel::result::Error> {
    use crate::schema::topics;
    let changeset_topic = topic.clone();
    diesel::insert_into(topics::table)
        .values(topic)
        .on_conflict((topics::message_thread_id, topics::chat_id))
        .do_update()
        .set(&changeset_topic)
        .get_result::<Topic>(conn)
}

// plays_without_topic returns all plays from the database that don't have an associated topic.
pub fn get_plays_without_topic(
    conn: &mut PgConnection,
    chat_id: i64,
) -> Result<Vec<(Play, Vec<Screening>)>, diesel::result::Error> {
    // Importing necessary methods
    use crate::schema::plays;
    use crate::schema::topics;

    // The subquery to find topics for a given play_id and chat_id
    let subquery = topics::table.filter(
        topics::play_id
            .eq(plays::id)
            .and(topics::chat_id.eq(chat_id)),
    );

    // The final query to find all plays that don't have an associated topic
    let plays: Vec<Play> = plays::table
        .filter(diesel::dsl::not(diesel::dsl::exists(subquery)))
        .load(conn)?;

    // get all screenings for all plays
    let screenings = Screening::belonging_to(&plays)
        .select(Screening::as_select())
        .load(conn)?;

    // group the screenings per play
    let screenings_per_play = screenings
        .grouped_by(&plays)
        .into_iter()
        .zip(plays)
        .map(|(screens, play)| (play, screens))
        .collect::<Vec<(Play, Vec<Screening>)>>();

    Ok(screenings_per_play)
}

pub fn get_plays_and_topics(
    conn: &mut PgConnection,
    chat_id: i64,
) -> Result<Vec<PlayAndTopic>, diesel::result::Error> {
    use crate::schema::{plays, screenings, topics};

    // First query the plays with their associated topics (if any)
    let results = plays::table
        .left_outer_join(topics::table.on(plays::id.eq(topics::play_id)))
        .filter(topics::chat_id.eq(chat_id).or(topics::chat_id.is_null()))
        .select((plays::all_columns, topics::all_columns.nullable()))
        .load::<(Play, Option<Topic>)>(conn)?;

    // Fetch screenings for all the plays
    let play_ids = results.iter().map(|(play, _)| play.id).collect::<Vec<_>>();
    let screenings = screenings::table
        .filter(screenings::play_id.eq_any(play_ids))
        .order_by(screenings::start_time.asc())
        .load::<Screening>(conn)?;

    // Group the screenings by play_id
    let mut screenings_map: HashMap<i32, Vec<Screening>> = HashMap::new();
    for screening in screenings {
        screenings_map
            .entry(screening.play_id)
            .and_modify(|screenings| screenings.push(screening.clone()))
            .or_insert_with(|| vec![screening.clone()]);
    }

    // Combine the plays, topics, and screenings
    Ok(results
        .into_iter()
        .map(|(play, topic)| {
            let screenings = screenings_map.get(&play.id).unwrap_or(&vec![]).clone();
            PlayAndTopic {
                play: PlayWithScreenings { play, screenings },
                topic,
            }
        })
        .collect())
}

pub fn create_play_with_screenings(
    conn: &mut PgConnection,
    play: PlayWithScreenings,
) -> Result<PlayWithScreenings, diesel::result::Error> {
    use crate::schema::plays;
    use crate::schema::screenings;

    let new_play: NewPlay = NewPlay {
        url: &play.play.url,
        name: &play.play.name,
        description: &play.play.description,
        image_url: &play.play.image_url,
        meta_info: &play.play.meta_info,
    };

    let changeset_play = new_play.clone();

    conn.build_transaction().read_write().run(|conn| {
        let new_play = diesel::insert_into(plays::table)
            .values(new_play)
            .on_conflict(plays::url)
            .do_update()
            .set(&changeset_play)
            .get_result::<Play>(conn)?;

        let screenings = play
            .screenings
            .iter()
            .map(|s| NewScreening {
                play_id: new_play.id,
                webid: &s.webid,
                location: &s.location,
                url: &s.url,
                start_time: s.start_time,
                ticket_url: &s.ticket_url,
            })
            .map(|s| {
                let changeset_screening = s.clone();
                diesel::insert_into(screenings::table)
                    .values(s)
                    .on_conflict(screenings::webid)
                    .do_update()
                    .set(&changeset_screening)
                    .get_result::<Screening>(conn)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PlayWithScreenings {
            play: new_play,
            screenings,
        })
    })
}
