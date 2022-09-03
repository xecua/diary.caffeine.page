use anyhow::{bail, Context};
use clap::{command, Arg};
use generator::generate;
use handlebars::handlebars_helper;
use state::State;
use std::{collections::HashMap, path::PathBuf, sync::Mutex};

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
            Arg::new("clean")
                .short('c')
                .long("clean")
                .help("Cleanup out_dir (cause force-overwrite)")
                .action(clap::ArgAction::SetTrue),
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

    let clean = matches.get_one::<bool>("clean").unwrap().to_owned();

    state::STATE
        .set(State {
            article_dir: article_dir.to_owned(),
            out_dir: out_dir.to_owned(),
            public_dir: public_dir.to_owned(),
            clean,
            blog_name: std::env::var("BLOG_NAME").unwrap_or("".to_string()),
            handlebars,
            opengraph_cache: Mutex::new(HashMap::new()),
        })
        .unwrap();

    generate()
}
