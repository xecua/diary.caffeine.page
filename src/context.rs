use serde_json::{Map, Value};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

#[derive(Debug)]
pub(crate) struct Context {
    pub article_dir: PathBuf,
    pub out_dir: PathBuf,
    pub public_dir: PathBuf,

    pub blog_name: String,
    pub blog_url: String,

    pub handlebars: handlebars::Handlebars<'static>,
    pub opengraph_cache: Mutex<Map<String, Value>>,
}

static CONTEXT: OnceLock<Context> = OnceLock::new();

impl Context {
    pub fn init(
        article_dir: PathBuf,
        out_dir: PathBuf,
        public_dir: PathBuf,
        blog_name: String,
        blog_url: String,
        handlebars: handlebars::Handlebars<'static>,
        opengraph_cache: Mutex<Map<String, Value>>,
    ) {
        CONTEXT
            .set(Self {
                article_dir,
                out_dir,
                public_dir,
                blog_name,
                blog_url,
                handlebars,
                opengraph_cache,
            })
            .unwrap();
    }

    pub fn instance() -> &'static Context {
        CONTEXT.get().unwrap()
    }
}
