use std::{borrow::Borrow, cmp::Ordering};

use log::{debug, warn};
use maud::html;
use pulldown_cmark::{Event, LinkType, Tag, TagEnd};
use serde_json::{json, Value};
use webpage::{Opengraph, OpengraphObject, Webpage, WebpageOptions};

use crate::state::State;

use super::data::ArticleMetadata;

pub(super) fn render_card(href: &str, og: &Opengraph) -> String {
    // TODO: change element by og-type
    html! {
        a.og-href href=(href) {
            span.og-card.{ "og-type-" (og.og_type) } {
                span.og-text {
                    span.og-title { (og.properties.get("title").unwrap()) }
                    span.og-desc { (og.properties.get("description").unwrap_or(&" ".to_string())) }
                    span.og-url { (og.properties.get("url").unwrap()) }
                    }
                span.og-image-wrap {
                    img.og-image src=(og.images[0].url);
                }
            }
        }
    }
    .into()
}

pub(super) fn sort_article<T: Borrow<ArticleMetadata>>(a: &T, b: &T) -> Ordering {
    match (a.borrow().date, b.borrow().date) {
        (Some(ref a_date), Some(ref b_date)) => b_date.cmp(a_date),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => b.borrow().title.cmp(&a.borrow().title),
    }
}

pub(super) fn gen_parser_event_iterator() -> Box<dyn FnMut(Event) -> Event> {
    let s = State::instance();
    let mut ogp_replacing = false;

    Box::new(move |event: Event| -> Event {
        // TODO: 数式とか?
        // debug!("{:?}", event);
        match event {
            Event::Start(Tag::Link {
                link_type: LinkType::Autolink,
                dest_url: ref url,
                ..
            }) => {
                // fetch OGP info
                // 内部リンクの場合自動的に(index).htmlを付与する、とかあった方が便利そうだな
                {
                    debug!("Getting cache of {url}...");
                    let cache = s.opengraph_cache.lock().unwrap();
                    if let Some(c) = cache.get(&url.to_string()) {
                        debug!("done.");
                        if *c != Value::Null {
                            let mut will_render_card = true;
                            let mut og = Opengraph::empty();
                            if let Some(Value::String(og_type)) = c.get("type") {
                                og.og_type = og_type.clone();
                            } else {
                                will_render_card = false;
                                warn!("Invalid cache (type does not exist): {}", c);
                            }
                            if let Some(Value::String(og_title)) = c.get("title") {
                                og.properties.insert("title".to_string(), og_title.clone());
                            } else {
                                will_render_card = false;
                                warn!("Invalid cache (title does not exist): {}", c);
                            }
                            if let Some(Value::String(og_url)) = c.get("url") {
                                og.properties.insert("url".to_string(), og_url.clone());
                            } else {
                                will_render_card = false;
                                warn!("Invalid cache (url does not exist): {}", c);
                            }
                            if let Some(Value::String(og_thumb_url)) = c.get("thumb_url") {
                                og.images = vec![OpengraphObject::new(og_thumb_url.clone())];
                            } else {
                                will_render_card = false;
                                warn!(
                                    "Invalid cache (thumbnail url(thumb_url) does not exist): {}",
                                    c
                                )
                            }
                            if let Some(Value::String(description)) = c.get("description") {
                                og.properties
                                    .insert("description".to_string(), description.clone());
                            }

                            if will_render_card {
                                ogp_replacing = true;
                                return Event::Html(render_card(url, &og).into());
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
                let mut options = WebpageOptions::default();
                // Hint from https://qiita.com/JunkiHiroi/items/f03d4297e11ce5db172e: this may be useful even for other than twitter
                options.useragent = "bot".to_string();

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
                        return Event::Html(render_card(url, &og).into());
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
            Event::End(TagEnd::Link) => {
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
