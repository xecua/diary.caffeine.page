use std::{
    collections::{HashMap, VecDeque},
    fs::{create_dir_all, Metadata as FileMetadata, OpenOptions},
    io::BufWriter,
    iter::FromIterator,
    path::PathBuf,
    rc::Rc,
    time::SystemTime,
};

use anyhow::{bail, Context};

use atom_syndication::{EntryBuilder, FeedBuilder, LinkBuilder};
use chrono::{Local, NaiveDate};

use fs_extra::dir::CopyOptions;
use log::debug;
use pulldown_cmark::{html, Options, Parser};

use self::{
    data::{ArticleMetadata, ArticlePageData, ListPageData},
    utils::{gen_parser_event_iterator, sort_article},
};
use crate::state::State;

mod data;
mod utils;

fn preprocess_article(
    file_relpath: PathBuf,
    file_meta: FileMetadata,
) -> anyhow::Result<ArticleMetadata> {
    let s = State::instance();
    let mut metadata = ArticleMetadata {
        title: "".to_string(),
        tags: vec![],
        date: None,
        relpath: file_relpath.with_extension(""),
        body: "".to_string(),
        file_meta,
    };

    let source_abspath = s.article_dir.join(&metadata.relpath.with_extension("md"));
    let content = std::fs::read_to_string(&source_abspath)
        .with_context(|| format!("while opening {:?}", source_abspath))?;
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

fn generate_article(metadata: &ArticleMetadata) -> anyhow::Result<()> {
    let s = State::instance();
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(&metadata.body, options).map(gen_parser_event_iterator());

    let out_abspath = s.out_dir.join(&metadata.relpath.with_extension("html"));

    create_dir_all(out_abspath.parent().unwrap())?;
    let out_abs_fd = OpenOptions::new()
        .write(true)
        .create(true)
        .open(&out_abspath)
        .with_context(|| format!("while opening file {:?}", out_abspath))?;

    let mut body_html = String::new();
    html::push_html(&mut body_html, parser);

    let data = ArticlePageData {
        blog_name: &s.blog_name,
        body: body_html,
        meta: metadata,
    };
    s.handlebars
        .render_to_write("article", &data, out_abs_fd)
        .with_context(|| format!("while generating {:?}", out_abspath))?;

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
    let mut directory_entries: HashMap<PathBuf, Vec<Rc<ArticleMetadata>>> = HashMap::new();
    let mut tags: HashMap<String, Vec<Rc<ArticleMetadata>>> = HashMap::new();

    // traversing `article_dir`
    let mut q = VecDeque::new(); // article_dirからの相対パスを入れるqueue
    q.push_back(PathBuf::new());
    while let Some(current_directory_relpath) = q.pop_front() {
        // relpathはarticle_dirからの相対パス、abspathはarticle_dirを含めたパス
        // abspathは厳密にはabsではないかもしれない
        let current_directory_abspath = s.article_dir.join(&current_directory_relpath);

        let entries_in_current_directory = directory_entries
            .entry(current_directory_relpath.clone())
            .or_default();

        for entry in std::fs::read_dir(current_directory_abspath)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            let entry_relpath = current_directory_relpath.join(&entry.file_name());

            if meta.is_dir() {
                q.push_back(entry_relpath.clone());

                (*entries_in_current_directory).push(Rc::new(ArticleMetadata {
                    title: entry.file_name().to_string_lossy().into_owned(),
                    tags: vec![],
                    date: None,
                    relpath: entry_relpath.clone(),
                    body: "".to_string(),
                    file_meta: meta,
                }));
            } else if meta.is_file() {
                let article_meta =
                    Rc::new(preprocess_article(entry_relpath, meta).with_context(|| {
                        format!(
                            "while preprocessing {:?}",
                            &current_directory_relpath.join(entry.file_name())
                        )
                    })?);
                for tag in article_meta.tags.iter() {
                    let tag_entries = tags.entry(tag.to_string()).or_default();
                    (*tag_entries).push(Rc::clone(&article_meta));
                    // (*tag_entries).push(articles.len());
                }
                (*entries_in_current_directory).push(Rc::clone(&article_meta));
                articles.push(article_meta.clone());
            }
        }
    }

    articles.sort_by(sort_article);

    debug!("generating articles");
    for article in articles.iter() {
        generate_article(article)?;
    }

    debug!("generating feed");
    {
        let offset: chrono::FixedOffset = chrono::FixedOffset::east_opt(60 * 60 * 9).unwrap();
        let channel = FeedBuilder::default()
            .title(format!("articles - {}", s.blog_name))
            .lang(Some("ja".to_string()))
            .links(vec![
                LinkBuilder::default()
                    .href(&s.blog_url)
                    .mime_type(Some("text/html".to_string()))
                    .build(),
                LinkBuilder::default()
                    .href(format!("{}/feed.atom", s.blog_url))
                    .mime_type(Some("application/atom+xml".to_string()))
                    .build(),
            ])
            .id(&s.blog_url) // RFC3987 IRI: 各ページのURLでいいんじゃないか
            .updated(Local::now().with_timezone(&offset))
            .entries(Vec::from_iter(articles.iter().map(|art| {
                let uri = format!("{}/{}", s.blog_url, art.path.to_string_lossy());

                EntryBuilder::default()
                    .title(&*art.title)
                    .link(
                        LinkBuilder::default()
                            .href(&uri)
                            .mime_type(Some("text/html".to_string()))
                            .build(),
                    )
                    .id(&uri)
                    .updated(
                        chrono::DateTime::<Local>::from(
                            art.file_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                        )
                        .with_timezone(&offset),
                    )
                    .build()
            })))
            .build();

        let feed_fd = OpenOptions::new()
            .create(true)
            .write(true)
            .open(s.out_dir.join("feed.atom"))
            .context("while opening feed file")?;
        let writer = BufWriter::new(feed_fd);
        channel.write_to(writer).context("while writing feed")?;
    }

    debug!("generating directory-index pages");
    for (directory_relpath, mut entries_in_current_directory) in directory_entries.into_iter() {
        let name = directory_relpath.to_string_lossy().to_string();
        let out_index_abspath = s.out_dir.join(&directory_relpath).join("index.html");
        entries_in_current_directory.sort_by(sort_article);

        create_dir_all(out_index_abspath.parent().unwrap()).with_context(|| {
            format!(
                "while making parent directories for {:?}",
                directory_relpath
            )
        })?;

        if name.is_empty() {
            // root
            let mut articles: Vec<&ArticleMetadata> =
                articles.iter().take(10).map(|a| a.as_ref()).collect();

            // 先頭10件が最新
            articles.append(
                &mut entries_in_current_directory
                    .iter()
                    .map(|e| e.as_ref())
                    .collect(),
            );

            let index_data = ListPageData {
                blog_name: &s.blog_name,
                title: "index".to_string(),
                relpath: PathBuf::from("/"),
                articles,
            };

            let out_index_fd = OpenOptions::new()
                .write(true)
                .create(true)
                .open(out_index_abspath)
                .context("while opening index.html")?;
            s.handlebars
                .render_to_write("index", &index_data, out_index_fd)
                .context("while generating index.html")?;
        } else {
            let list_data = ListPageData {
                blog_name: &s.blog_name,
                title: name,
                relpath: directory_relpath,
                articles: entries_in_current_directory
                    .iter()
                    .map(|e| e.as_ref())
                    .collect(),
            };

            let out_index_fd = OpenOptions::new()
                .write(true)
                .create(true)
                .open(out_index_abspath)
                .with_context(|| format!("while opening list for {:?}", list_data.title))?;
            s.handlebars
                .render_to_write("list", &list_data, out_index_fd)
                .with_context(|| format!("while generating list for {:?}", list_data.title))?;
        }
    }

    debug!("generating tag-index pages");
    create_dir_all(s.out_dir.join("tags"))
        .context("while making parent directories for tags page")?;
    for (tag, mut tag_articles) in tags.into_iter() {
        let tag_relpath = PathBuf::from("tags").join(&tag);
        let out_abspath = s.out_dir.join(&tag_relpath.with_extension("html"));

        tag_articles.sort_by(sort_article);

        let list_data = ListPageData {
            blog_name: &s.blog_name,
            title: format!("タグ: {}", tag),
            relpath: tag_relpath,
            articles: tag_articles.iter().map(|a| a.as_ref()).collect(),
        };

        let out_tag_fd = OpenOptions::new()
            .write(true)
            .create(true)
            .open(out_abspath)?;
        s.handlebars
            .render_to_write("list", &list_data, out_tag_fd)
            .with_context(|| format!("while generating for tag {:?}", tag))?;
    }

    Ok(())
}
