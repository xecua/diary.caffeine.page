use std::{
    collections::{HashMap, VecDeque},
    fs::OpenOptions,
    path::PathBuf,
};

use anyhow::{bail, Context};

use chrono::NaiveDate;
use either::Either;
use fs_extra::dir::CopyOptions;
use log::debug;
use pulldown_cmark::{html, Event, Options, Parser};
use serde::Serialize;

use crate::{metadata::Metadata, state::State};

#[derive(Serialize, Debug)]
struct Data {
    blog_name: String,
    body: String,
    title: String,
    tags: Vec<String>,
    path: PathBuf,
    date: chrono::NaiveDate,
}

fn process_file(metadata: &Metadata) -> anyhow::Result<()> {
    let s = State::instance();
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(&metadata.body, options).map(|event| {
        // TODO: 数式とか
        debug!("{:?}", event);
        match event {
            Event::SoftBreak => Event::HardBreak,
            _ => event,
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

    let data = Data {
        blog_name: std::env::var("BLOG_NAME").unwrap_or("".to_string()),
        body: body_html,
        path: metadata.path.clone(),
        title: metadata.title.clone(),
        tags: metadata.tags.clone(),
        date: metadata.date.clone(),
    };
    s.handlebars
        .render_to_write("article", &data, fd)
        .with_context(|| format!("while generating from {:?}", metadata.path))?;
    Ok(())
}

fn preprocess_file(file_path: &PathBuf) -> anyhow::Result<Metadata> {
    let s = State::instance();
    let path = s.article_dir.join(file_path);

    let mut file_path_html: PathBuf = file_path.clone();
    file_path_html.set_extension("html");

    let mut metadata = Metadata {
        title: "".to_string(),
        tags: vec![],
        date: NaiveDate::default(),
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
                    metadata.date = NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .context("Invalid date format")?;
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

#[derive(Serialize, Debug)]
struct IndexData<'a> {
    blog_name: String,
    title: String,
    path: PathBuf,
    articles: &'a Vec<Metadata>,
}

#[derive(Serialize, Debug)]
struct TagData<'a> {
    blog_name: String,
    title: String,
    tag: String,
    path: PathBuf,
    articles: Vec<&'a Metadata>,
}

pub(crate) fn generate() -> anyhow::Result<()> {
    let s = State::instance();

    // backup
    fs_extra::dir::remove(&s.out_dir)?;

    let mut cp_opts = CopyOptions::new();
    cp_opts.copy_inside = true;
    cp_opts.content_only = true;
    cp_opts.overwrite = true;
    fs_extra::dir::copy(&s.public_dir, s.out_dir.join(&s.public_dir), &cp_opts)?;

    let mut articles = vec![];
    let mut dir_ents: HashMap<PathBuf, Vec<Either<usize, Metadata>>> = HashMap::new(); // right: directory
    let mut tags: HashMap<String, Vec<usize>> = HashMap::new();
    let mut q = VecDeque::new();
    q.push_back(PathBuf::new());
    while let Some(path) = q.pop_front() {
        let dirname = s.article_dir.join(&path);
        let dir_entries = dir_ents.entry(path.clone()).or_default();
        for entry in std::fs::read_dir(dirname)? {
            let entry = entry?;
            let meta = entry.metadata()?;

            if meta.is_dir() {
                let entry_path = path.join(entry.file_name());
                let entry_name = entry.file_name().to_string_lossy().to_string();
                q.push_back(entry_path.clone());
                (*dir_entries).push(Either::Right(Metadata {
                    title: entry_name,
                    tags: vec![],
                    date: NaiveDate::default(),
                    path: entry_path,
                    body: "".to_string(),
                }));
            } else if meta.is_file() {
                let article_meta =
                    preprocess_file(&path.join(entry.file_name())).with_context(|| {
                        format!("while preprocessing {:?}", &path.join(entry.file_name()))
                    })?;
                for tag in article_meta.tags.iter() {
                    let entry = tags.entry(tag.to_string()).or_default();
                    (*entry).push(articles.len());
                }
                (*dir_entries).push(Either::Left(articles.len()));
                articles.push(article_meta.clone());
            }
        }
    }

    // index
    let index_fd = OpenOptions::new()
        .write(true)
        .create(true)
        .open(s.out_dir.join("index.html"))?;
    let index_data = IndexData {
        blog_name: std::env::var("BLOG_NAME").unwrap_or_default(),
        title: "index".to_string(),
        path: PathBuf::from("/"),
        articles: &articles,
    };
    s.handlebars
        .render_to_write("index", &index_data, index_fd)
        .context("while generating index.html")?;

    // 各ページの生成
    for article in articles.iter() {
        process_file(article)?;
    }

    for (dir_name, entry) in dir_ents.into_iter() {
        // トップページだけ例外
        if dir_name == PathBuf::new() {
            continue;
        }

        let path = s.out_dir.join(&dir_name).join("index.html");
        let title = dir_name.to_string_lossy().to_string();
        let fd = OpenOptions::new().write(true).create(true).open(path)?;
        let data = TagData {
            blog_name: std::env::var("BLOG_NAME").unwrap_or_default(),
            title: title.clone(),
            path: dir_name,
            tag: "".to_string(), // 強引……
            articles: entry
                .iter()
                .map(|e| match e {
                    Either::Left(idx) => &articles[*idx],
                    Either::Right(meta) => meta,
                })
                .collect(),
        };
        s.handlebars
            .render_to_write("index", &data, fd)
            .with_context(|| format!("while generating list for {:?}", title))?;
    }

    fs_extra::dir::create_all(s.out_dir.join("tags"), false)?;
    for (tag, article_indices) in tags.into_iter() {
        let mut path = s.out_dir.join("tags").join(&tag);
        path.set_extension("html");
        let fd = OpenOptions::new().write(true).create(true).open(path)?;
        let data = TagData {
            blog_name: std::env::var("BLOG_NAME").unwrap_or_default(),
            title: format!("Tag: {}", tag),
            path: PathBuf::from("/tags").join(&tag),
            tag,
            articles: article_indices
                .into_iter()
                .map(|idx| &articles[idx])
                .collect(),
        };
        s.handlebars
            .render_to_write("tag", &data, fd)
            .with_context(|| format!("while generating for tag {:?}", data.tag))?;
    }

    Ok(())
}
