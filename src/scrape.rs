use crate::models::Play;
use crate::models::PlayWithScreenings;
use crate::models::Screening;
use anyhow::{anyhow, Context, Result};
use chrono::NaiveDateTime;
use lazy_static::lazy_static;
use log::error;
use regex::Regex;
use reqwest;
use scraper::ElementRef;
use scraper::{Html, Selector};
use std::collections::HashMap;
use std::error::Error;
#[allow(unused_imports)]
use std::fs::File;
#[allow(unused_imports)]
use std::io::Read;
#[allow(unused_imports)]
use std::io::Write;
use time::OffsetDateTime;
use time::UtcOffset;

const BASE_URL: &str = "https://www.schauspielhaus.ch";

lazy_static! {
    // Select the screening rows on the play page.
    static ref SCREENING_SELECTOR: Selector = Selector::parse("div.activity-snippet").unwrap();
    // Select the meta info of the play (duration, subtitles, etc.) on the play page.
    static ref METAINFO_SELECTOR: Selector = Selector::parse("ul.infos-column__list li").unwrap();
    // Select the play titles on the calendar page.
    static ref PLAY_TITLE_SELECTOR: Selector = Selector::parse("a.activity__title").unwrap();
}

pub fn get_plays() -> Result<HashMap<String, PlayWithScreenings>> {
    // base url
    let kalender_url = format!("{}/de/kalender", BASE_URL);

    // Make a GET request and retrieve the HTML content, on error log the error and return None
    let html_content = reqwest::blocking::get(&kalender_url)
        .context("loading main calendar page")?
        .text()
        .context("reading main calendar page")?;

    // Parse the HTML content with the scraper library
    let fragment = Html::parse_document(&html_content);

    // Iterate over all elements matching the given selector and add them to a map[url]Play.
    // For each Play just fill in the name (the inner HTML of the <a> tag) and the URL (the href of the <a> tag).
    let mut plays: HashMap<String, PlayWithScreenings> = HashMap::new();
    for element in fragment.select(&PLAY_TITLE_SELECTOR) {
        let name = element.inner_html();
        let url = match element.value().attr("href") {
            Some(url) => url.to_string(),
            None => {
                error!("No href attribute found for element {}", name);
                continue;
            }
        };

        let p = match get_play(&url) {
            Ok(p) => p,
            Err(e) => {
                error!(
                    "Error while requesting play info {}: {}",
                    url,
                    e.to_string()
                );
                continue;
            }
        };
        plays.insert(url.clone(), p);
    }
    Ok(plays)
}

pub fn get_play<'a>(url: &str) -> Result<PlayWithScreenings, Box<dyn Error>> {
    let mut play = PlayWithScreenings {
        play: Play {
            id: 0,
            url: url.to_string(),
            name: "".to_string(),
            description: "".to_string(),
            image_url: "".to_string(),
            meta_info: "".to_string(),
        },
        screenings: Vec::new(),
    };

    let play_page_content = reqwest::blocking::get(format!("{}{}", BASE_URL, url))?.text()?;
    let fragment = Html::parse_document(&play_page_content);

    // Get meta info (text that is to the left of the screening times)
    play.play.meta_info = fragment
        .select(&METAINFO_SELECTOR)
        .fold("".to_string(), |acc, element| {
            format!("{}\n{}", acc, element.inner_html())
        });

    let mut collect_screening = |production_row: ElementRef| -> Result<Screening> {
        // Search for `a.calendar-icon` in the production row
        let selector = Selector::parse("a.calendar-icon").unwrap();
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
                ("DESCRIPTION", Some(d)) => play.play.description = d,
                ("SUMMARY", Some(s)) => play.play.name = s,
                (_, _) => continue,
            }
        }
        let mut screening: Screening;
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
        // search for div.activity-snippet__date in the production row to extract the location of the screening from the
        // location string e.g. Di 03.10. 18:30 Schiffbau
        let selector = Selector::parse("div.activity-snippet__date").unwrap();
        let screening_info = production_row
            .select(&selector)
            .next()
            .context("No screening info found for element")?
            .inner_html();
        let re = Regex::new(r"\d{2}:\d{2}\s(.*)").unwrap();
        if let Some(captured) = re.captures(&screening_info) {
            if let Some(m) = captured.get(1) {
                screening.location = m.as_str().to_string();
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
    let selector = Selector::parse("img.production__heroimage").unwrap();
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
