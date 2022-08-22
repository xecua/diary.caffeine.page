use std::{fs::OpenOptions, path::PathBuf};

use anyhow::bail;

use fs_extra::dir::CopyOptions;
use log::debug;
use manifest_dir_macros::path;
use pulldown_cmark::{html, Event, Options, Parser};
use serde::Serialize;

use crate::{metadata::Metadata, state::State};

#[derive(Serialize, Debug)]
struct Data {
    blog_name: String,
    title: String,
    tags: Vec<String>,
    body: String,
}

fn process_file(file_path: &PathBuf) -> anyhow::Result<Metadata> {
    let s = State::instance();
    let path = s.article_dir.join(file_path);
    let content = std::fs::read_to_string(path)?;
    let mut metadata = Metadata {
        title: "".to_string(),
        tags: vec![],
    };

    // parsing pandoc-style metadata block
    let header_pattern = regex::RegexBuilder::new(r"^---\r?\n(.*)---\r?\n(.*)")
        .dot_matches_new_line(true)
        .build()
        .unwrap();
    let body = if let Some(caps) = header_pattern.captures(&content) {
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
                _ => {}
            }
        }

        caps[2].to_string()
    } else {
        content
    };

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(&body, options).map(|event| {
        // TODO: 数式とか
        debug!("{:?}", event);
        match event {
            Event::SoftBreak => Event::HardBreak,
            _ => event,
        }
    });

    // out
    let mut out_path = s.out_dir.join(file_path);
    out_path.set_extension("html");
    if out_path.parent().map_or(false, |p| !p.exists()) {
        std::fs::create_dir_all(out_path.parent().unwrap())?;
    }
    let fd = OpenOptions::new().write(true).create(true).open(out_path)?;

    let mut body_html = String::new();
    html::push_html(&mut body_html, parser);

    let data = Data {
        blog_name: std::env::var("BLOG_NAME").unwrap_or("".to_string()),
        title: metadata.title.clone(),
        tags: metadata.tags.clone(),
        body: body_html,
    };
    s.handlebars.render_to_write("article", &data, fd)?;

    Ok(metadata)
}

fn traverse_directory(path: &PathBuf) -> anyhow::Result<()> {
    // dir: relative path, article_dir: base (article_dir.join(dir) become current directory)
    let s = State::instance();
    let dirname = s.article_dir.join(path);
    for entry in std::fs::read_dir(dirname)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_dir() {
            traverse_directory(&path.join(entry.file_name()))?;
        } else if meta.is_file() {
            process_file(&path.join(entry.file_name()))?;
        }
    }

    Ok(())
}

pub(crate) fn generate() -> anyhow::Result<()> {
    // backup
    fs_extra::dir::remove(path!("out"))?;

    let mut cp_opts = CopyOptions::new();
    cp_opts.copy_inside = true;
    cp_opts.content_only = true;
    cp_opts.overwrite = true;
    fs_extra::dir::copy(path!("public"), path!("out", "public"), &cp_opts)?;

    traverse_directory(&PathBuf::from("."))?;

    Ok(())
}
