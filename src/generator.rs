use std::{
    cmp::Ordering,
    collections::{HashMap, VecDeque},
    fs::OpenOptions,
    path::PathBuf,
};

use anyhow::{bail, Context};

use chrono::NaiveDate;
use either::Either;
use fs_extra::dir::CopyOptions;
use log::{debug, warn};
use pulldown_cmark::{html, Event, Options, Parser, Tag};
use serde::Serialize;
use serde_json::{json, Value};
use webpage::{Opengraph, OpengraphObject, Webpage, WebpageOptions};

use crate::{metadata::Metadata, state::State};

#[derive(Serialize, Debug)]
struct ArticlePageData<'a> {
    blog_name: &'static str,
    body: String,
    meta: &'a Metadata,
}

#[derive(Serialize, Debug)]
struct ListPageData<'a> {
    blog_name: &'static str,
    title: String,
    path: PathBuf,
    articles: Vec<&'a Metadata>,
}

fn preprocess_file(file_path: &PathBuf) -> anyhow::Result<Metadata> {
    let s = State::instance();
    let path = s.article_dir.join(file_path);

    let mut file_path_html: PathBuf = file_path.clone();
    file_path_html.set_extension("html");

    let mut metadata = Metadata {
        title: "".to_string(),
        tags: vec![],
        date: None,
        path: file_path_html,
        body: "".to_string(),
    };

    let content = std::fs::read_to_string(&path)?;
    // parsing pandoc-style metadata block
    let header_pattern = regex::RegexBuilder::new(r"^---\r?\n(.*)---\r?\n(.*)")
        .dot_matches_new_line(true)
        .build()
        .unwrap();
    metadata.body = if let Some(caps) = header_pattern.captures(&content) {
        let header = &caps[1];
        for line in header.split('\n') {
            if line.is_empty() {
                continue;
            }
            let s: Vec<_> = line.split(':').collect();
            if s.len() != 2 {
                bail!("Invalid header: {}", line);
            }

            let name = s[0].trim();
            let value = s[1].trim();
            // currently, title and tag are supported
            match name {
                "title" => {
                    metadata.title = value.to_string();
                }
                "tag" => {
                    metadata.tags = value.split(',').map(|s| s.to_string()).collect();
                }
                "date" => {
                    metadata.date = Some(
                        NaiveDate::parse_from_str(value, "%Y-%m-%d")
                            .context("Invalid date format")?,
                    );
                }
                _ => {}
            }
        }

        caps[2].to_string()
    } else {
        content
    };

    Ok(metadata)
}

fn render_card(og: &Opengraph) -> String {
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

fn generate_article(metadata: &Metadata) -> anyhow::Result<()> {
    let s = State::instance();
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let mut ogp_replacing = false;

    let parser = Parser::new_ext(&metadata.body, options).map(|event| {
        // TODO: 数式とか?
        // debug!("{:?}", event);
        match event {
            Event::Start(Tag::Link(pulldown_cmark::LinkType::Autolink, ref url, _)) => {
                // fetch OGP info
                {
                    ogp_replacing = true;

                    let cache = s.opengraph_cache.lock().unwrap();
                    if let Some(c) = cache.get(&url.to_string()) {
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
                        }
                    }
                }
                // there is no cache: try to fetch
                let options = WebpageOptions {
                    // Hint from https://qiita.com/JunkiHiroi/items/f03d4297e11ce5db172e: this may be useful even for other than twitter
                    useragent: "bot".to_string(),
                    ..Default::default()
                };

                if let Ok(webpage) = Webpage::from_url(url, options) {
                    std::thread::sleep(std::time::Duration::from_secs(10));

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
    });

    // out
    let mut out_abs_path = s.out_dir.join(&metadata.path);
    out_abs_path.set_extension("html");

    if out_abs_path.parent().map_or(false, |p| !p.exists()) {
        std::fs::create_dir_all(out_abs_path.parent().unwrap())?;
    }
    let out_abs_fd = OpenOptions::new()
        .write(true)
        .create(true)
        .open(out_abs_path)?;

    let mut body_html = String::new();
    html::push_html(&mut body_html, parser);

    let data = ArticlePageData {
        blog_name: &s.blog_name,
        body: body_html,
        meta: metadata,
    };
    s.handlebars
        .render_to_write("article", &data, out_abs_fd)
        .with_context(|| format!("while generating from {:?}", metadata.path))?;

    Ok(())
}

fn sort_article(a: &&Metadata, b: &&Metadata) -> Ordering {
    match (a.date, b.date) {
        (Some(ref a_date), Some(ref b_date)) => b_date.cmp(a_date),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => b.title.cmp(&a.title),
    }
}

pub(crate) fn generate() -> anyhow::Result<()> {
    let s = State::instance();

    fs_extra::dir::remove(&s.out_dir)?;

    // copy `public_dir`
    let mut cp_opts = CopyOptions::new();
    cp_opts.copy_inside = true;
    cp_opts.content_only = true;
    cp_opts.overwrite = true;
    fs_extra::dir::copy(&s.public_dir, s.out_dir.join(&s.public_dir), &cp_opts)?;

    // master data
    let mut articles = vec![];

    // subdirectory data
    // left: index of `articles` / right: directory(pseudo entry data)
    let mut directories: HashMap<PathBuf, Vec<Either<usize, Metadata>>> = HashMap::new();
    let mut tags: HashMap<String, Vec<usize>> = HashMap::new();

    // traversing `article_dir`
    let mut q = VecDeque::new();
    q.push_back(PathBuf::new());
    while let Some(path) = q.pop_front() {
        let current_searching_directory_path = s.article_dir.join(&path);

        let entries_in_current_path = directories.entry(path.clone()).or_default();

        for entry in std::fs::read_dir(current_searching_directory_path)? {
            let entry = entry?;
            let meta = entry.metadata()?;

            if meta.is_dir() {
                let directory_path = path.join(entry.file_name());
                q.push_back(directory_path.clone());

                let directory_name = entry.file_name().to_string_lossy().to_string();
                (*entries_in_current_path).push(Either::Right(Metadata {
                    title: directory_name,
                    tags: vec![],
                    date: None,
                    path: directory_path,
                    body: "".to_string(),
                }));
            } else if meta.is_file() {
                let article_meta =
                    preprocess_file(&path.join(entry.file_name())).with_context(|| {
                        format!("while preprocessing {:?}", &path.join(entry.file_name()))
                    })?;
                for tag in article_meta.tags.iter() {
                    let tag_entries = tags.entry(tag.to_string()).or_default();
                    (*tag_entries).push(articles.len());
                }
                (*entries_in_current_path).push(Either::Left(articles.len()));
                articles.push(article_meta.clone());
            }
        }
    }

    debug!("generating articles");
    for article in articles.iter() {
        generate_article(article)?;
    }

    debug!("generating directory-index pages");
    for (out_rel_dir_path, entry) in directories.into_iter() {
        let out_abs_file_path = s.out_dir.join(&out_rel_dir_path).join("index.html");
        // index page
        let out_rel_dir_name = out_rel_dir_path.to_string_lossy().to_string();

        if out_rel_dir_name.is_empty() {
            // ordering by date(descending). if both are directory, compare by directory name.
            let mut articles: Vec<&Metadata> = articles.iter().collect();
            articles.sort_by(sort_article);

            let index_data = ListPageData {
                blog_name: &s.blog_name,
                title: "index".to_string(),
                path: PathBuf::from("/"),
                articles,
            };

            let out_abs_index_fd = OpenOptions::new()
                .write(true)
                .create(true)
                .open(out_abs_file_path)?;
            s.handlebars
                .render_to_write("index", &index_data, out_abs_index_fd)
                .context("while generating index.html")?;
        } else {
            // ordering by date(descending). if both are directory, compare by directory name.
            let mut articles: Vec<&Metadata> = entry
                .iter()
                .map(|e| match e {
                    Either::Left(idx) => &articles[*idx],
                    Either::Right(meta) => meta,
                })
                .collect();
            articles.sort_by(sort_article);

            let list_data = ListPageData {
                blog_name: &s.blog_name,
                title: out_rel_dir_name,
                path: out_rel_dir_path,
                articles,
            };

            let out_abs_file_fd = OpenOptions::new()
                .write(true)
                .create(true)
                .open(out_abs_file_path)?;
            s.handlebars
                .render_to_write("list", &list_data, out_abs_file_fd)
                .with_context(|| format!("while generating list for {:?}", list_data.title))?;
        }
    }

    debug!("generating tag-index pages");
    fs_extra::dir::create_all(s.out_dir.join("tags"), false)?;
    for (tag, article_indices) in tags.into_iter() {
        let out_rel_file_path = PathBuf::from("tags").join(&tag);
        let mut out_abs_file_path = s.out_dir.join(&out_rel_file_path);
        out_abs_file_path.set_extension("html");

        // ordering by date(descending). if both are directory, compare by directory name.
        let mut articles: Vec<&Metadata> = article_indices
            .into_iter()
            .map(|idx| &articles[idx])
            .collect();
        articles.sort_by(sort_article);

        let list_data = ListPageData {
            blog_name: &s.blog_name,
            title: format!("タグ: {}", tag),
            path: out_rel_file_path,
            articles,
        };

        let abs_abs_file_fd = OpenOptions::new()
            .write(true)
            .create(true)
            .open(out_abs_file_path)?;
        s.handlebars
            .render_to_write("list", &list_data, abs_abs_file_fd)
            .with_context(|| format!("while generating for tag {:?}", tag))?;
    }

    Ok(())
}
