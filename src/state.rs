use std::{collections::HashMap, path::PathBuf, sync::Mutex};

use once_cell::sync::OnceCell;
use webpage::Opengraph;

#[derive(Debug)]
pub(crate) struct State {
    pub article_dir: PathBuf,
    pub out_dir: PathBuf,
    pub public_dir: PathBuf,

    pub clean: bool,

    pub blog_name: String,

    pub handlebars: handlebars::Handlebars<'static>,
    pub opengraph_cache: Mutex<HashMap<String, Option<Opengraph>>>,
}

pub(super) static STATE: OnceCell<State> = OnceCell::new();

impl State {
    pub fn instance() -> &'static State {
        STATE.get().unwrap()
    }
}
