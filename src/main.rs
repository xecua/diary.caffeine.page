use anyhow::bail;
use clap::{command, Arg};
use generator::generate;
use manifest_dir_macros::{file_path, path};
use state::State;
use std::path::PathBuf;

mod generator;
mod metadata;
mod state;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let matches = command!()
        .args(&[
            Arg::new("article_dir")
                .help("Directory path of articles")
                .takes_value(true)
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(path!("posts")),
            Arg::new("out_dir")
                .help("Directory path of output. Existing contents will be removed.")
                .takes_value(true)
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(path!("out")),
            // TODO: template
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

    let mut handlebars = handlebars::Handlebars::new();
    handlebars.register_template_file("article", file_path!("template/article.hbs"))?;
    handlebars.register_partial("header", include_str!(file_path!("template/header.hbs")))?;
    handlebars.register_partial("side", include_str!(file_path!("template/side.hbs")))?;

    state::STATE
        .set(State {
            article_dir: article_dir.to_owned(),
            out_dir: out_dir.to_owned(),
            handlebars,
        })
        .unwrap();

    generate()
}
