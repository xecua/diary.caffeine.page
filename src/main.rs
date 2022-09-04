use anyhow::{bail, Context};
use clap::{command, Arg};
use generator::generate;
use handlebars::handlebars_helper;
use serde_json::Map;
use state::State;
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter},
    path::PathBuf,
    sync::Mutex,
};

mod generator;
mod metadata;
mod state;

handlebars_helper!(breadcrumbs: |path: PathBuf| {
    let mut current_path = PathBuf::from("/");
    let mut res = String::new();
    let mut components = path.components();
    if path.has_root() {
        components.next();
    }
    res.push_str("<a href=\"/\">/</a> ");
    for (i, c) in components.enumerate() {
        current_path.push(c);
        res.push_str(
            &format!("{}<a href=\"{}\">{}</a>",
            if i == 0 {""} else {" / "},
            current_path.to_string_lossy(),
            current_path.file_stem().unwrap().to_string_lossy() // file_prefix: unstable
        ));
    }

    res
});

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let matches = command!()
        .args(&[
            Arg::new("article_dir")
                .help("Directory path of articles")
                .takes_value(true)
                .value_parser(clap::value_parser!(PathBuf))
                .default_value("posts"),
            Arg::new("out_dir")
                .help("Directory path of output. Existing contents will be removed.")
                .takes_value(true)
                .value_parser(clap::value_parser!(PathBuf))
                .default_value("out"),
            Arg::new("public_dir")
                .help("Directory path of public. Contents will be copied as it is.")
                .takes_value(true)
                .value_parser(clap::value_parser!(PathBuf))
                .default_value("public"),
            Arg::new("template_dir")
                .help("Directory of template")
                .takes_value(true)
                .value_parser(clap::value_parser!(PathBuf))
                .default_value("template"),
        ])
        .get_matches();

    let article_dir: &PathBuf = matches.get_one("article_dir").unwrap();
    if !article_dir.exists() || !article_dir.is_dir() {
        bail!("article_dir must be a directory.");
    }
    let out_dir: &PathBuf = matches.get_one("out_dir").unwrap();
    if out_dir.exists() && !out_dir.is_dir() {
        bail!("if out_dir exists, it must be directory.");
    }
    let public_dir: &PathBuf = matches.get_one("public_dir").unwrap();
    if !public_dir.exists() || !public_dir.is_dir() {
        bail!("public_dir must be a directory.")
    }

    let template_dir: &PathBuf = matches.get_one("template_dir").unwrap();
    if !template_dir.exists() || !template_dir.is_dir() {
        bail!("template_dir must be a directory.")
    }
    let mut handlebars = handlebars::Handlebars::new();
    handlebars.register_helper("breadcrumbs", Box::new(breadcrumbs));
    handlebars
        .register_template_file("index", template_dir.join("index.hbs"))
        .context("index.hbs")?;
    handlebars
        .register_template_file("article", template_dir.join("article.hbs"))
        .context("article.hbs")?;
    handlebars
        .register_template_file("list", template_dir.join("list.hbs"))
        .context("list.hbs")?;
    handlebars.register_partial(
        "layout",
        std::fs::read_to_string(template_dir.join("layout.hbs")).context("header.hbs")?,
    )?;

    let cache_json_path = PathBuf::from("cache.json");
    let cache_data = {
        if cache_json_path.exists() {
            let fd = File::open(&cache_json_path)?;
            let reader = BufReader::new(fd);
            serde_json::from_reader(reader)?
        } else {
            Map::new()
        }
    };

    state::STATE
        .set(State {
            article_dir: article_dir.to_owned(),
            out_dir: out_dir.to_owned(),
            public_dir: public_dir.to_owned(),
            blog_name: std::env::var("BLOG_NAME").unwrap_or("".to_string()),
            handlebars,
            opengraph_cache: Mutex::new(cache_data),
        })
        .unwrap();

    generate()?;

    // save cache
    {
        let cache_json_fd = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&cache_json_path)?;
        let writer = BufWriter::new(cache_json_fd);
        let cache = state::STATE.get().unwrap().opengraph_cache.lock().unwrap();
        serde_json::to_writer_pretty(writer, &*cache)?;
    }

    Ok(())
}
