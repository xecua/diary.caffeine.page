use clap::{command, Arg};
use manifest_dir_macros::path;
use std::path::PathBuf;

fn main() {
    let matches = command!()
        .args(&[
            Arg::new("article_dir")
                .help("Directory path of articles")
                .takes_value(true)
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(path!("posts")),
            Arg::new("out_dir")
                .help("Directory path of output")
                .takes_value(true)
                .value_parser(clap::value_parser!(PathBuf))
                .default_value(path!("out")),
            // TODO: template
        ])
        .get_matches();

    let article_dir: &PathBuf = matches.get_one("article_dir").unwrap();
    let out_dir: &PathBuf = matches.get_one("out_dir").unwrap();

    println!("{article_dir:?} {out_dir:?}");
}
