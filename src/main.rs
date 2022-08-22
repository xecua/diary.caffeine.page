use anyhow::bail;
use clap::{command, Arg};
use generator::generate;
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
    handlebars.register_template_file("index", template_dir.join("index.hbs"))?;
    handlebars.register_template_file("article", template_dir.join("article.hbs"))?;
    handlebars.register_template_file("tag", template_dir.join("tag.hbs"))?;
    handlebars.register_partial(
        "header",
        std::fs::read_to_string(template_dir.join("header.hbs"))?,
    )?;
    handlebars.register_partial(
        "side",
        std::fs::read_to_string(template_dir.join("side.hbs"))?,
    )?;

    state::STATE
        .set(State {
            article_dir: article_dir.to_owned(),
            out_dir: out_dir.to_owned(),
            public_dir: public_dir.to_owned(),
            handlebars,
        })
        .unwrap();

    generate()
}
