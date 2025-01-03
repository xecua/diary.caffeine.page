pub(crate) use crate::{context::Context, generator::generate};
use anyhow::bail;
use cache::{load_cache, save_cache};
use clap::{command, Arg};
use std::{path::PathBuf, sync::Mutex};

mod cache;
mod context;
mod generator;
mod renderer;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let matches = command!()
        .args([
            Arg::new("article_dir")
                .help("Directory path of articles")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value("posts"),
            Arg::new("out_dir")
                .help("Directory path of output. Existing contents will be removed.")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value("out"),
            Arg::new("public_dir")
                .help("Directory path of public. Contents will be copied as it is.")
                .value_parser(clap::value_parser!(PathBuf))
                .default_value("public"),
            Arg::new("template_dir")
                .help("Directory of template")
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

    let cache_file_path = PathBuf::from("cache.json.zst");
    Context::init(
        article_dir.to_owned(),
        out_dir.to_owned(),
        public_dir.to_owned(),
        std::env::var("BLOG_NAME").unwrap_or_default(),
        std::env::var("BLOG_URL").unwrap_or_default(),
        handlebars,
        Mutex::new(load_cache(&cache_file_path)?),
    );

    generate()?;

    // save cache
    save_cache(
        &cache_file_path,
        &context::Context::instance().opengraph_cache.lock().unwrap(),
    )?;

    Ok(())
}
