use crate::models::PlayWithScreenings;
use crate::models::Screening;
use anyhow::{anyhow, Context, Result};
use chrono::NaiveDateTime;
use lazy_static::lazy_static;
use log::error;
use reqwest;
use scraper::ElementRef;
use scraper::{Html, Selector};
use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
#[allow(unused_imports)]
use std::fs::File;
#[allow(unused_imports)]
use std::io::Read;
#[allow(unused_imports)]
use std::io::Write;
use time::OffsetDateTime;
use time::UtcOffset;

pub const BASE_URL: &str = "https://www.schauspielhaus.ch";

// Prefix that all play titles have in common.
pub const TITLE_PREFIX: &str = "Schauspielhaus Zürich: ";

lazy_static! {
    // Select the screening rows on the play page.
    static ref SCREENING_SELECTOR: Selector = Selector::parse("div.article-event").unwrap();
    // Select the meta info of the play (duration, subtitles, etc.) on the play page.
    static ref METAINFO_SELECTOR: Selector = Selector::parse("div.article-content__info").unwrap();
    // Select the play titles on the calendar page.
    static ref PLAY_CALENDAR_TITLES_SELECTOR: Selector = Selector::parse("a.calendar-item__title").unwrap();
    // Select the play title on the play page.
    static ref PLAY_TITLE_SELECTOR: Selector = Selector::parse("h1.article__title").unwrap();
    // Select the play description on the play page.
    static ref PLAY_DESCRIPTION_SELECTOR: Selector = Selector::parse("div.article-content__text p").unwrap();
}

pub fn download_calendar() -> Result<String> {
    let kalender_url = format!("{}/de/kalender", BASE_URL);
    let html_content = reqwest::blocking::get(&kalender_url)
        .context("loading main calendar page")?
        .text()
        .context("reading main calendar page")?;
    Ok(html_content)
}

#[test]
fn test_download_calendar() {
    let html_content = download_calendar().unwrap();
    // write content to testdata/calendar.html file
    let mut file = File::create("src/testdata/calendar.html").unwrap();
    file.write_all(html_content.as_bytes()).unwrap();
}

fn find_plays(html_content: &str) -> Vec<String> {
    let fragment = Html::parse_document(html_content);
    let mut plays: HashSet<String> = HashSet::new();
    for element in fragment.select(&PLAY_CALENDAR_TITLES_SELECTOR) {
        let raw_name = element.inner_html();
        let name = raw_name.trim();
        let url = match element.value().attr("href") {
            Some(url) => url.to_string(),
            None => {
                error!("No href attribute found for element {}", name);
                continue;
            }
        };
        plays.insert(url.clone());
    }
    plays.into_iter().collect()
}

#[test]
fn test_find_plays() {
    let mut file = File::open("src/testdata/calendar.html").unwrap();
    let mut html_content = String::new();
    file.read_to_string(&mut html_content).unwrap();
    let plays = find_plays(&html_content);
    // json marshall plays to string with indentation
    let plays_json = serde_json::to_string_pretty(&plays).unwrap();
    goldie::assert!(plays_json);
}

// get_plays downloads a the plays from the schauspielhaus calendar
// and returns a map title -> PlayWithScreenings.
pub fn get_plays() -> Result<HashMap<String, PlayWithScreenings>> {
    // base url
    let html_content = download_calendar()?;

    let plays = find_plays(&html_content);
    let mut plays_with_screenings: HashMap<String, PlayWithScreenings> = HashMap::new();
    for play in plays {
        let p = match get_play(&play) {
            Ok(p) => p,
            Err(e) => {
                error!(
                    "Error while requesting play info {}: {}",
                    play,
                    e.to_string()
                );
                continue;
            }
        };
        plays_with_screenings.insert(play.clone(), p);
    }
    Ok(plays_with_screenings)
}

#[test]
fn test_download_play() {
    let play = &find_plays(&download_calendar().unwrap())[0];
    let play_page_content = reqwest::blocking::get(format!("{}{}", BASE_URL, play))
        .unwrap()
        .text()
        .unwrap();
    goldie::assert!(play_page_content);
}

#[test]
fn test_find_play_with_screenings() {
    // read html from src/testdata/test_download_play.golden
    let mut file = File::open("src/testdata/test_download_play.golden").unwrap();
    let mut html_content = String::new();
    file.read_to_string(&mut html_content).unwrap();
    let play = find_play_with_screenings("/de/play/der-zerbrochne-krug", &html_content).unwrap();
    let play_json = serde_json::to_string_pretty(&play).unwrap();
    goldie::assert!(play_json);
}

pub fn find_play_with_screenings(
    url: &str,
    play_page_content: &str,
) -> Result<PlayWithScreenings, Box<dyn Error>> {
    let mut play = PlayWithScreenings::default();
    play.play.url = url.to_string();

    let fragment = Html::parse_document(&play_page_content);

    play.play.name = fragment
        .select(&PLAY_TITLE_SELECTOR)
        .next()
        .map(|element| {
            element
                .text()
                .collect::<String>()
                .replace("\n", " ")
                .trim()
                .to_string()
        })
        .unwrap_or("".to_string());

    play.play.description = fragment
        .select(&PLAY_DESCRIPTION_SELECTOR)
        .map(|element| {
            element
                .text()
                .collect::<String>()
                .replace("\n", " ")
                .trim()
                .to_string()
        })
        .collect::<Vec<String>>()
        .join("\n");

    // Get meta info (text that is to the left of the screening times)
    play.play.meta_info = fragment
        .select(&METAINFO_SELECTOR)
        .filter_map(|element| {
            let mut text = element.inner_html();
            match text.split_once("</span>") {
                Some((_, t)) => text = t.to_string(),
                None => (),
            }
            text = text.trim().to_string();
            if text == "" {
                None
            } else {
                Some(text)
            }
        })
        .collect::<Vec<String>>()
        .join("\n");

    let collect_screening = |production_row: ElementRef| -> Result<Screening> {
        // Search for `a.calendar-icon` in the production row
        let selector = Selector::parse("div.activity-ticket__calendar a").unwrap();
        // Extract the calendar event link
        let calendar_link = production_row
            .select(&selector)
            .next()
            .context("No calendar icon found for element")?
            .value()
            .attr("href")
            .context("error finding href attribute for element")?
            .to_string();

        // Download ics file at the calendar link and parse the contents to extract
        // Description, start and end date.
        let buf = reqwest::blocking::get(format!("{}{}", BASE_URL, calendar_link))?.bytes()?;
        let reader = ical::PropertyParser::from_reader(buf.as_ref());
        let mut id: Option<String> = None;
        let mut start: Option<OffsetDateTime> = None;

        for l in reader {
            let line = l?;
            match (line.name.as_str(), line.value) {
                ("UID", Some(i)) => id = Some(i),
                ("DTSTART", Some(d)) => start = parse_time(d),
                (_, _) => continue,
            }
        }
        let screening: Screening;
        match (id, start) {
            (Some(i), Some(s)) => {
                screening = Screening {
                    id: 0,
                    play_id: 0,
                    url: calendar_link,
                    location: "".to_string(),
                    webid: i,
                    start_time: s,
                }
            }
            (i, s) => {
                return Err(anyhow!(
                    "error filling screening link: {}, id {:?}, start {:?}",
                    calendar_link,
                    i,
                    s,
                ));
            }
        }
        Ok(screening)
    };

    for production_row in fragment.select(&SCREENING_SELECTOR) {
        match collect_screening(production_row) {
            Ok(s) => play.screenings.push(s),
            Err(e) => {
                error!("Error collecting screening: {}", e.to_string());
            }
        }
    }

    // Get Production image
    let selector = Selector::parse("div.article__hero img").unwrap();
    for element in fragment.select(&selector) {
        play.play.image_url = match element.value().attr("data-src") {
            Some(url) => url.to_string(),
            None => {
                error!("No src attribute found for image element {}", url);
                continue;
            }
        };
        break;
    }

    // Find Location
    Ok(play)
}

pub fn get_play<'a>(url: &str) -> Result<PlayWithScreenings, Box<dyn Error>> {
    let play_page_content = reqwest::blocking::get(format!("{}{}", BASE_URL, url))?.text()?;
    find_play_with_screenings(url, &play_page_content)
}

fn parse_time(d: String) -> Option<OffsetDateTime> {
    let t = match NaiveDateTime::parse_from_str(d.as_str(), "%Y%m%dT%H%M%S") {
        Ok(t) => t,
        Err(e) => {
            error!("error parsing time: {}", e.to_string());
            return None;
        }
    };
    let datetime = OffsetDateTime::from_unix_timestamp(t.timestamp()).unwrap();
    Some(datetime.replace_offset(UtcOffset::from_whole_seconds(7200).unwrap()))
}

#[test]
fn test_screenings_selector() {
    for (path, expected) in [("testdata/play.html", 2), ("testdata/play_curl.html", 14)] {
        let mut file = File::open(path).unwrap();
        let mut html_content = String::new();
        file.read_to_string(&mut html_content).unwrap();

        let fragment = Html::parse_document(&html_content);
        assert_eq!(
            fragment.select(&SCREENING_SELECTOR).count(),
            expected,
            "path: {}",
            path
        );
    }
}

#[test]
fn test_get_plays() {
    let plays = get_plays().unwrap();
    assert_eq!(plays.len(), 10);
}

#[test]
fn test_metainfo_selector() {
    let path = "testdata/play_curl.html";
    let mut file = File::open(path).unwrap();
    let mut html_content = String::new();
    file.read_to_string(&mut html_content).unwrap();
    let fragment = Html::parse_document(&html_content);
    let information = fragment
        .select(&METAINFO_SELECTOR)
        .fold("".to_string(), |acc, element| {
            format!("{}\n{}", acc, element.inner_html())
        });
    goldie::assert!(information)
}
