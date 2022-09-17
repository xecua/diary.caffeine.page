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
use pulldown_cmark::{html, Options, Parser};

use self::{
    data::{ArticlePageData, ListPageData, Metadata},
    utils::{gen_parser_event_iterator, sort_article},
};
use crate::state::State;

mod data;
mod utils;

fn preprocess_article(file_path: &PathBuf) -> anyhow::Result<Metadata> {
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

fn generate_article(metadata: &Metadata) -> anyhow::Result<()> {
    let s = State::instance();
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(&metadata.body, options).map(gen_parser_event_iterator());

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
                    preprocess_article(&path.join(entry.file_name())).with_context(|| {
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
