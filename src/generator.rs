use std::{
    collections::{HashMap, VecDeque},
    fs::OpenOptions,
    path::PathBuf,
};

use anyhow::{bail, Context};

use chrono::NaiveDate;
use either::Either;
use fs_extra::dir::CopyOptions;
use pulldown_cmark::{html, Event, Options, Parser, Tag};
use serde::Serialize;
use webpage::{Opengraph, Webpage, WebpageOptions};

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
        for line in header.split("\n") {
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
                    metadata.tags = value.split(",").map(|s| s.to_string()).collect();
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
                if !url.ends_with(":card") && !url.ends_with(":fast_card") {
                    return event;
                }

                let (wait_sec, url) = if url.ends_with(":fast_card") {
                    (1, &url[..(url.len() - 10)]) // for apparently powerful website
                } else {
                    (10, &url[..(url.len() - 5)])
                };

                // fetch OGP info
                {
                    if let Some(Some(og)) = s.opengraph_cache.lock().unwrap().get(url) {
                        ogp_replacing = true;
                        return Event::Html(render_card(og).into());
                    }
                }
                // there is no cache: try to fetch
                let options = WebpageOptions {
                    // Hint from https://qiita.com/JunkiHiroi/items/f03d4297e11ce5db172e: this may be useful even for other than twitter
                    useragent: "bot".to_string(),
                    ..Default::default()
                };

                if let Ok(webpage) = Webpage::from_url(&url, options) {
                    std::thread::sleep(std::time::Duration::from_secs(wait_sec));

                    // OGP Requirements: title, type, url, image. So convert into card only if all of them exist
                    let og = webpage.html.opengraph;
                    if !og.og_type.is_empty()
                        && og.properties.contains_key("title")
                        && og.properties.contains_key("url")
                        && og.images.len() >= 1
                    {
                        // caching.
                        {
                            s.opengraph_cache
                                .lock()
                                .unwrap()
                                .insert(url.to_string(), Some(og.clone()));
                        }
                        ogp_replacing = true;
                        return Event::Html(render_card(&og).into());
                    }
                }
                // no need to caching (because there is no ogp info, nor the webpage did not exist.)
                s.opengraph_cache
                    .lock()
                    .unwrap()
                    .insert(url.to_string(), None);
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
    let mut out_path = s.out_dir.join(&metadata.path);
    out_path.set_extension("html");
    if out_path.parent().map_or(false, |p| !p.exists()) {
        std::fs::create_dir_all(out_path.parent().unwrap())?;
    }
    let fd = OpenOptions::new().write(true).create(true).open(out_path)?;

    let mut body_html = String::new();
    html::push_html(&mut body_html, parser);

    let data = ArticlePageData {
        blog_name: &s.blog_name,
        body: body_html,
        meta: &metadata,
    };
    s.handlebars
        .render_to_write("article", &data, fd)
        .with_context(|| format!("while generating from {:?}", metadata.path))?;
    Ok(())
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

    // generate article pages
    for article in articles.iter() {
        generate_article(article)?;
    }

    // generate index page
    {
        let index_fd = OpenOptions::new()
            .write(true)
            .create(true)
            .open(s.out_dir.join("index.html"))?;

        // ordering by date(descending). if both are directory, compare by directory name.
        let mut articles: Vec<&Metadata> = articles.iter().collect();
        articles.sort_by(|a, b| match (a.date, b.date) {
            (Some(ref a_date), Some(ref b_date)) => b_date.cmp(a_date),
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => b.title.cmp(&a.title),
        });

        let index_data = ListPageData {
            blog_name: &s.blog_name,
            title: "index".to_string(),
            path: PathBuf::from("/"),
            articles,
        };
        s.handlebars
            .render_to_write("index", &index_data, index_fd)
            .context("while generating index.html")?;
    }

    // generate directory index pages
    for (dir_name, entry) in directories.into_iter() {
        // index page
        if dir_name == PathBuf::new() {
            continue;
        }

        let path = s.out_dir.join(&dir_name).join("index.html");
        let title = dir_name.to_string_lossy().to_string();
        let fd = OpenOptions::new().write(true).create(true).open(path)?;

        // ordering by date(descending). if both are directory, compare by directory name.
        let mut articles: Vec<&Metadata> = entry
            .iter()
            .map(|e| match e {
                Either::Left(idx) => &articles[*idx],
                Either::Right(meta) => meta,
            })
            .collect();
        articles.sort_by(|a, b| match (a.date, b.date) {
            (Some(ref a_date), Some(ref b_date)) => b_date.cmp(a_date),
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => b.title.cmp(&a.title),
        });

        let data = ListPageData {
            blog_name: &s.blog_name,
            title: title.clone(),
            path: dir_name,
            articles,
        };
        s.handlebars
            .render_to_write("list", &data, fd)
            .with_context(|| format!("while generating list for {:?}", title))?;
    }

    // generate tag pages
    fs_extra::dir::create_all(s.out_dir.join("tags"), false)?;
    for (tag, article_indices) in tags.into_iter() {
        let mut path = s.out_dir.join("tags").join(&tag);
        path.set_extension("html");
        let fd = OpenOptions::new().write(true).create(true).open(path)?;

        // ordering by date(descending). if both are directory, compare by directory name.
        let mut articles: Vec<&Metadata> = article_indices
            .into_iter()
            .map(|idx| &articles[idx])
            .collect();
        articles.sort_by(|a, b| match (a.date, b.date) {
            (Some(ref a_date), Some(ref b_date)) => b_date.cmp(a_date),
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => b.title.cmp(&a.title),
        });

        let data = ListPageData {
            blog_name: &s.blog_name,
            title: format!("タグ: {}", tag),
            path: PathBuf::from("/tags").join(&tag),
            articles,
        };
        s.handlebars
            .render_to_write("list", &data, fd)
            .with_context(|| format!("while generating for {:?}", data.title))?;
    }

    Ok(())
}
