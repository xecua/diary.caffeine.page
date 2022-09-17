use anyhow::{bail, Context};
use clap::{command, Arg};
use generator::generate;
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
mod renderer;
mod state;

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

    let handlebars = renderer::generate_renderer(template_dir)?;

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
