use std::cmp::Ordering;

use log::{debug, warn};
use pulldown_cmark::{Event, LinkType, Tag};
use serde_json::{json, Value};
use webpage::{Opengraph, OpengraphObject, Webpage, WebpageOptions};

use crate::state::State;

use super::data::ArticleMetadata;

pub(super) fn render_card(og: &Opengraph) -> String {
    // TODO: change element by og_type
    format!(
        concat!(
            "<a class=\"og-href\" href=\"{}\">",
            "  <span class=\"og-card og_type_{}\">",
            "    <span class=\"og-text\">",
            "      <span class=\"og-title\">{}</span>",
            "      <span class=\"og-desc\">{}</span>",
            "      <span class=\"og-url\">{}</span>",
            "    </span>",
            "    <span class=\"og-image-wrap\">",
            "      <img class=\"og-image\" src=\"{}\">",
            "    </span>",
            "  </span>",
            "</a>",
        ),
        og.properties.get("url").unwrap(),
        og.og_type,
        og.properties.get("title").unwrap(),
        og.properties.get("description").unwrap_or(&" ".to_string()),
        og.properties.get("url").unwrap(),
        og.images[0].url
    )
}

pub(super) fn sort_article(a: &&ArticleMetadata, b: &&ArticleMetadata) -> Ordering {
    match (a.date, b.date) {
        (Some(ref a_date), Some(ref b_date)) => b_date.cmp(a_date),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => b.title.cmp(&a.title),
    }
}

pub(super) fn gen_parser_event_iterator() -> Box<dyn FnMut(Event) -> Event> {
    let s = State::instance();
    let mut ogp_replacing = false;

    Box::new(move |event: Event| -> Event {
        // TODO: 数式とか?
        // debug!("{:?}", event);
        match event {
            Event::Start(Tag::Link(LinkType::Autolink, ref url, _)) => {
                // fetch OGP info
                {
                    ogp_replacing = true;

                    debug!("Getting cache of {url}...");
                    let cache = s.opengraph_cache.lock().unwrap();
                    if let Some(c) = cache.get(&url.to_string()) {
                        debug!("done.");
                        if *c != Value::Null {
                            let mut og = Opengraph::empty();
                            if let Some(Value::String(og_type)) = c.get("type") {
                                og.og_type = og_type.clone();
                            } else {
                                ogp_replacing = false;
                                warn!("Invalid cache (type does not exist): {}", c);
                            }
                            if let Some(Value::String(og_title)) = c.get("title") {
                                og.properties.insert("title".to_string(), og_title.clone());
                            } else {
                                ogp_replacing = false;
                                warn!("Invalid cache (title does not exist): {}", c);
                            }
                            if let Some(Value::String(og_url)) = c.get("url") {
                                og.properties.insert("url".to_string(), og_url.clone());
                            } else {
                                ogp_replacing = false;
                                warn!("Invalid cache (url does not exist): {}", c);
                            }
                            if let Some(Value::String(og_thumb_url)) = c.get("thumb_url") {
                                og.images = vec![OpengraphObject::new(og_thumb_url.clone())];
                            } else {
                                ogp_replacing = false;
                                warn!(
                                    "Invalid cache (thumbnail url(thumb_url) does not exist): {}",
                                    c
                                )
                            }
                            if let Some(Value::String(description)) = c.get("description") {
                                og.properties
                                    .insert("description".to_string(), description.clone());
                            }

                            if ogp_replacing {
                                // あんまり行儀がよくない
                                return Event::Html(render_card(&og).into());
                            }
                        } else {
                            // nullの時returnするの忘れてたな……
                            debug!("but there seemed to be no ogp info.");
                            return event;
                        }
                    }
                }

                debug!("failed. fetching...");
                // there is no cache: try to fetch
                let options = WebpageOptions {
                    // Hint from https://qiita.com/JunkiHiroi/items/f03d4297e11ce5db172e: this may be useful even for other than twitter
                    useragent: "bot".to_string(),
                    ..Default::default()
                };

                if let Ok(webpage) = Webpage::from_url(url, options) {
                    std::thread::sleep(std::time::Duration::from_secs(10));
                    debug!("done.");

                    // OGP Requirements: title, type, url, image. So convert into card only if all of them exist
                    let og = webpage.html.opengraph;
                    if !og.og_type.is_empty()
                        && og.properties.contains_key("title")
                        && og.properties.contains_key("url")
                        && !og.images.is_empty()
                    {
                        // caching.
                        {
                            let mut cache = s.opengraph_cache.lock().unwrap();
                            cache.insert(
                                url.to_string(),
                                if og.properties.contains_key("description") {
                                    json!({
                                        "type": og.og_type,
                                        "title": og.properties["title"],
                                        "url": og.properties["url"],
                                        "description": og.properties["description"],
                                        "thumb_url": og.images[0].url
                                    })
                                } else {
                                    json!({
                                        "type": og.og_type,
                                        "title": og.properties["title"],
                                        "url": og.properties["url"],
                                        "thumb_url": og.images[0].url
                                    })
                                },
                            );
                        }
                        ogp_replacing = true;
                        return Event::Html(render_card(&og).into());
                    }
                }
                // no need to caching (because there is no ogp info, nor the webpage did not exist.)
                {
                    debug!("there was no ogp info.");
                    let mut cache = s.opengraph_cache.lock().unwrap();
                    cache.insert(url.to_string(), Value::Null);
                }
                event
            }
            Event::End(Tag::Link(pulldown_cmark::LinkType::Autolink, _, _)) => {
                if ogp_replacing {
                    ogp_replacing = false;
                    Event::Text("".into())
                } else {
                    event
                }
            }
            Event::SoftBreak => Event::HardBreak,
            _ => {
                if ogp_replacing {
                    Event::Text("".into())
                } else {
                    event
                }
            }
        }
    })
}
