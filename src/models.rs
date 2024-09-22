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
}

// Implement std::fmt::Display for Screening
impl std::fmt::Display for Screening {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let format = format_description::parse("[day].[month].[year] [hour]:[minute]").unwrap();
        write!(
            f,
            "[{}]({}{})",
            self.start_time.format(&format).unwrap(),
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

pub struct PlayWithChats {
    pub play: Play,
    pub chats: Vec<ChatWithTopics>,
}

#[derive(Default)]
pub struct ChatWithTopics {
    pub chat: Chat,
    pub topics: Vec<Topic>,
}

pub fn get_chat_with_topics(
    conn: &mut PgConnection,
    chat_id: i64,
) -> Result<ChatWithTopics, diesel::result::Error> {
    use crate::schema::chats;
    use crate::schema::topics;

    let chat = chats::table.find(chat_id).first::<Chat>(conn)?;

    let topics = topics::table
        .filter(topics::chat_id.eq(chat_id))
        .load::<Topic>(conn)?;

    Ok(ChatWithTopics { chat, topics })
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
    use crate::schema::topics;

    let topic = topics::table.find(topic_id).first::<Topic>(conn)?;

    let play = plays::table.find(topic.play_id).first::<Play>(conn)?;

    let screenings = Screening::belonging_to(&play)
        .select(Screening::as_select())
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

pub fn put_topic(conn: &mut PgConnection, topic: Topic) -> Result<Topic, diesel::result::Error> {
    use crate::schema::topics;
    let changeset_topic = topic.clone();
    diesel::insert_into(topics::table)
        .values(topic)
        .on_conflict(topics::message_thread_id)
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
