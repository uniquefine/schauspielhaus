use diesel::prelude::*;
use time::OffsetDateTime;

#[derive(Queryable, Identifiable, Selectable, Debug, PartialEq)]
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

#[derive(Queryable, Selectable, Identifiable, Associations, Debug, PartialEq, Clone)]
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

pub struct PlayWithScreenings {
    pub play: Play,
    pub screenings: Vec<Screening>,
}

pub struct NewPlayWithScreenings<'a> {
    pub play: NewPlay<'a>,
    pub screenings: Vec<NewScreening<'a>>,
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
