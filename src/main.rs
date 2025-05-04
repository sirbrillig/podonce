use chrono::NaiveDateTime;
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::CONTENT_LENGTH;
use std::error::Error;
use std::fs::File;
use xml::writer::{EmitterConfig, XmlEvent};

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

fn get_html() -> String {
    let response = reqwest::blocking::get("https://wmbr.org/cgi-bin/arch");
    response.unwrap().text().unwrap()
}

fn get_episodes(html_content: &str) -> Vec<Episode> {
    let document = scraper::Html::parse_document(html_content);
    let episode_selector = scraper::Selector::parse(".fbody5 tr").unwrap();
    let episode_title_selector = scraper::Selector::parse("td:first-of-type").unwrap();
    let episode_date_selector = scraper::Selector::parse("td:nth-of-type(2)").unwrap();
    let episode_url_selector = scraper::Selector::parse("td:nth-of-type(3) a").unwrap();
    let episode_year_selector = scraper::Selector::parse("td:nth-of-type(3) a.archives").unwrap();
    let date_format = "%a %b %d %I:%M %P %Y";
    let year_re = Regex::new(r"_(\d{4})\d+$").unwrap();
    let file_name_re = Regex::new(r"\('([^']+)'").unwrap();
    let url_path_prefix = "http://wmbr.org/archive/";
    let mut episodes: Vec<Episode> = Vec::new();
    let html_episodes = document.select(&episode_selector);
    for html_episode in html_episodes {
        let title_element = html_episode.select(&episode_title_selector).next();
        let title_text = match title_element {
            Some(title_element) => title_element.text().collect::<Vec<_>>().join(""),
            None => continue,
        };
        let date_element = html_episode.select(&episode_date_selector).next();
        let date_text = match date_element {
            Some(date_element) => date_element.text().collect::<Vec<_>>().join(""),
            None => continue,
        };
        let year_element = html_episode.select(&episode_year_selector).next();
        let year_text = match year_element {
            Some(year_element) => match year_element.attr("id") {
                Some(year_text) => year_text,
                None => {
                    continue;
                }
            },
            None => continue,
        };
        let year_captures = match year_re.captures(year_text) {
            Some(year_captures) => year_captures,
            None => continue,
        };
        let year = &year_captures[1];
        if year.is_empty() {
            continue;
        }
        let date_year = year;
        let date_text_with_year = date_text + " " + date_year;
        let date = match NaiveDateTime::parse_from_str(&date_text_with_year, &date_format) {
            Ok(date) => date,
            Err(_) => {
                continue;
            }
        };
        let url_element = html_episode.select(&episode_url_selector).next();
        let url_onclick_text = match url_element {
            Some(url_element) => match url_element.attr("onclick") {
                Some(url_onclick_text) => url_onclick_text,
                None => {
                    continue;
                }
            },
            None => continue,
        };
        let file_name_captures = match file_name_re.captures(url_onclick_text) {
            Some(file_name_captures) => file_name_captures,
            None => continue,
        };
        let file_name = &file_name_captures[1];
        if file_name.is_empty() {
            continue;
        }
        let url_text = url_path_prefix.to_owned() + file_name;
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

fn write_podcast_xml(podcast: &Podcast, file_path: &str) -> std::io::Result<()> {
    let file = File::create(file_path)?;
    let mut writer = EmitterConfig::new()
        .perform_indent(true)
        .create_writer(file);

    writer
        .write(XmlEvent::start_element("rss").attr("version", "2.0"))
        .unwrap();
    writer.write(XmlEvent::start_element("channel")).unwrap();
    // FIXME: add atom:link (see https://validator.w3.org/feed/docs/warning/MissingAtomSelfLink.html)

    writer.write(XmlEvent::start_element("title")).unwrap();
    writer.write(XmlEvent::characters(&podcast.title)).unwrap();
    writer.write(XmlEvent::end_element()).unwrap();
    writer.write(XmlEvent::start_element("link")).unwrap();
    writer.write(XmlEvent::characters(&podcast.link)).unwrap();
    writer.write(XmlEvent::end_element()).unwrap();
    writer
        .write(XmlEvent::start_element("description"))
        .unwrap();
    writer
        .write(XmlEvent::characters(&podcast.description))
        .unwrap();
    writer.write(XmlEvent::end_element()).unwrap();

    for episode in &podcast.episodes {
        writer.write(XmlEvent::start_element("item")).unwrap();
        writer.write(XmlEvent::start_element("title")).unwrap();
        writer.write(XmlEvent::characters(&episode.title)).unwrap();
        writer.write(XmlEvent::end_element()).unwrap(); // end title
        writer.write(XmlEvent::start_element("guid")).unwrap();
        writer.write(XmlEvent::characters(&episode.url)).unwrap();
        writer.write(XmlEvent::end_element()).unwrap(); // end guid
        writer.write(XmlEvent::start_element("pubDate")).unwrap();
        writer
            .write(XmlEvent::characters(
                &episode.date.format("%a, %d %b %Y %H:%M:%S EST").to_string(),
            ))
            .unwrap();
        writer.write(XmlEvent::end_element()).unwrap(); // end pubDate
        writer
            .write(
                XmlEvent::start_element("enclosure")
                    .attr("url", &episode.url)
                    .attr("length", &episode.length.to_string())
                    .attr("type", "audio/mpeg"),
            )
            .unwrap();
        writer.write(XmlEvent::end_element()).unwrap(); // end enclosure
        writer.write(XmlEvent::end_element()).unwrap(); // end item
    }

    writer.write(XmlEvent::end_element()).unwrap();
    writer.write(XmlEvent::end_element()).unwrap();

    Ok(())
}
