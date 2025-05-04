use chrono::NaiveDateTime;
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::CONTENT_LENGTH;
use std::error::Error;
use std::fs::File;
use xml::writer::{EmitterConfig, EventWriter, XmlEvent};
use scraper::{Selector, ElementRef};

#[derive(Debug)]
struct Episode {
    date: NaiveDateTime,
    title: String,
    url: String,
    length: usize,
}

#[derive(Debug)]
struct Podcast {
    title: String,
    link: String,
    description: String,
    episodes: Vec<Episode>,
}

struct Constants {
    episode_selector: Selector,
    episode_title_selector: Selector,
    episode_date_selector: Selector,
    episode_url_selector: Selector,
    episode_year_selector: Selector,
    date_format: &'static str,
    year_re: Regex,
    file_name_re: Regex,
    url_path_prefix: &'static str,
}

fn main() {
    let episodes = get_episodes(&get_html());
    let podcast = Podcast {
        title: "WMBR Archive".into(),
        link: "https://wmbr.org/cgi-bin/arch".into(),
        description: "The most recent WMBR episodes".into(),
        episodes,
    };
    write_podcast_xml(&podcast, "wmbr.xml").unwrap();
}

fn prepare_constants() -> Constants {
    Constants {
    episode_selector : scraper::Selector::parse(".fbody5 tr").unwrap(),
    episode_title_selector : scraper::Selector::parse("td:first-of-type").unwrap(),
    episode_date_selector : scraper::Selector::parse("td:nth-of-type(2)").unwrap(),
    episode_url_selector : scraper::Selector::parse("td:nth-of-type(3) a").unwrap(),
    episode_year_selector : scraper::Selector::parse("td:nth-of-type(3) a.archives").unwrap(),
    date_format : "%a %b %d %I:%M %P %Y",
    year_re : Regex::new(r"_(\d{4})\d+$").unwrap(),
    file_name_re : Regex::new(r"\('([^']+)'").unwrap(),
    url_path_prefix : "http://wmbr.org/archive/",
    }
}

fn get_html() -> String {
    let response = reqwest::blocking::get("https://wmbr.org/cgi-bin/arch");
    response.unwrap().text().unwrap()
}

fn get_title_from_html(constants: &Constants, html_episode: ElementRef) -> Result<String, Box<dyn Error>> {
        let title_element = html_episode.select(&constants.episode_title_selector).next();
         match title_element {
            Some(title_element) => Ok(title_element.text().collect::<Vec<_>>().join("")),
            None => Err(Box::from("Failed to find title in html")),
        }
}

fn get_date_from_html(constants: &Constants, html_episode: ElementRef) -> Result<NaiveDateTime, Box<dyn Error>> {
        let date_element = html_episode.select(&constants.episode_date_selector).next();
        let date_text = match date_element {
            Some(date_element) => date_element.text().collect::<Vec<_>>().join(""),
                None => return Err(Box::from("Failed to find date in html")),
        };
        let year_element = html_episode.select(&constants.episode_year_selector).next();
        let year_text = match year_element {
            Some(year_element) => match year_element.attr("id") {
                Some(year_text) => year_text,
                None => return Err(Box::from("Failed to find year ID in html")),
            },
            None => return Err(Box::from("Failed to find year in html")),
        };
        let year_captures = match constants.year_re.captures(year_text) {
            Some(year_captures) => year_captures,
            None => return Err(Box::from("Failed to find year number in year text")),
        };
        let year = &year_captures[1];
        if year.is_empty() {
            return Err(Box::from("Year number was empty in year text"));
        }
        let date_year = year;
        let date_text_with_year = date_text + " " + date_year;
        Ok(NaiveDateTime::parse_from_str(&date_text_with_year, &constants.date_format)?)
}

fn get_episodes(html_content: &str) -> Vec<Episode> {
    let document = scraper::Html::parse_document(html_content);
    let mut episodes: Vec<Episode> = Vec::new();
    let constants = prepare_constants();
    let html_episodes = document.select(&constants.episode_selector);
    for html_episode in html_episodes {
        let title_text = match get_title_from_html(&constants, html_episode) {
            Ok(x) => x,
            Err(_) => continue,
        };
        let date = match get_date_from_html(&constants, html_episode) {
            Ok(x) => x,
            Err(_) => continue,
        };
        let url_element = html_episode.select(&constants.episode_url_selector).next();
        let url_onclick_text = match url_element {
            Some(url_element) => match url_element.attr("onclick") {
                Some(url_onclick_text) => url_onclick_text,
                None => {
                    continue;
                }
            },
            None => continue,
        };
        let file_name_captures = match constants.file_name_re.captures(url_onclick_text) {
            Some(file_name_captures) => file_name_captures,
            None => continue,
        };
        let file_name = &file_name_captures[1];
        if file_name.is_empty() {
            continue;
        }
        let url_text = constants.url_path_prefix.to_owned() + file_name;
        let length = match get_file_length(&url_text) {
            Ok(length) => length,
            Err(_) => continue,
        };
        let episode = Episode {
            title: title_text,
            date,
            url: url_text,
            length,
        };
        episodes.push(episode);
    }
    episodes
}

/// Return the file length in bytes
fn get_file_length(url: &str) -> Result<usize, Box<dyn Error>> {
    let client = Client::new();

    let response = client.head(url).send()?;

    if let Some(content_length) = response.headers().get(CONTENT_LENGTH) {
        if let Ok(content_length_str) = content_length.to_str() {
            if let Ok(content_length) = content_length_str.parse::<usize>() {
                return Ok(content_length);
            } else {
                return Err(Box::from("Failed to parse content length."));
            }
        } else {
            return Err(Box::from("Couldn't convert content length to str."));
        }
    }
    return Err(Box::from("Content length header not found."));
}

/// Return the approximate number of seconds for the episode
fn get_duration_for_episode(episode: &Episode) -> usize {
    // First convert the file size from bytes to bits.
    let length_bits = episode.length * 8;
    // Assume bitrate is 128.
    let bitrate_kbps = 128;
    let bitrate_bps = bitrate_kbps * 1000;
    length_bits / bitrate_bps
}

fn format_date_as_rfc2822(date: &NaiveDateTime) -> String {
    date.format("%a, %d %b %Y %H:%M:%S EST").to_string()
}

fn write_open_tag(mut writer: EventWriter<File>, name: &str) -> EventWriter<File> {
    writer.write(XmlEvent::start_element(name)).unwrap();
    writer
}

fn write_close_tag(mut writer: EventWriter<File>, _name: &str) -> EventWriter<File> {
    writer.write(XmlEvent::end_element()).unwrap();
    writer
}

fn write_tag(mut writer: EventWriter<File>, name: &str, content: &str) -> EventWriter<File> {
    writer = write_open_tag(writer, name);
    writer.write(XmlEvent::characters(content)).unwrap();
    writer = write_close_tag(writer, name);
    writer
}

fn write_audio_tag(mut writer: EventWriter<File>, episode: &Episode) -> EventWriter<File> {
    writer
        .write(
            XmlEvent::start_element("enclosure")
                .attr("url", &episode.url)
                .attr("length", &episode.length.to_string())
                .attr("type", "audio/mpeg"),
        )
        .unwrap();
    writer = write_close_tag(writer, "enclosure");
    writer
}

fn write_episode(mut writer: EventWriter<File>, episode: &Episode) -> EventWriter<File> {
    writer = write_open_tag(writer, "item");
    writer = write_tag(writer, "title", &episode.title);
    writer = write_tag(writer, "guid", &episode.url);
    writer = write_tag(
        writer,
        "itunes:duration",
        &get_duration_for_episode(&episode).to_string(),
    );
    writer = write_tag(writer, "pubDate", &format_date_as_rfc2822(&episode.date));
    writer = write_audio_tag(writer, &episode);
    writer = write_close_tag(writer, "item");
    writer
}

fn write_podcast_xml(podcast: &Podcast, file_path: &str) -> std::io::Result<()> {
    let file = File::create(file_path)?;
    let mut writer = EmitterConfig::new()
        .perform_indent(true)
        .create_writer(file);

    writer
        .write(XmlEvent::start_element("rss").attr("version", "2.0"))
        .unwrap();
    writer = write_open_tag(writer, "channel");

    writer = write_tag(writer, "title", &podcast.title);
    writer = write_tag(writer, "link", &podcast.link);
    writer
        .write(XmlEvent::start_element("description"))
        .unwrap();
    writer
        .write(XmlEvent::characters(&podcast.description))
        .unwrap();
    writer.write(XmlEvent::end_element()).unwrap();

    for episode in &podcast.episodes {
        writer = write_episode(writer, &episode)
    }

    writer = write_close_tag(writer, "channel");
    writer.write(XmlEvent::end_element()).unwrap(); // end rss

    Ok(())
}
