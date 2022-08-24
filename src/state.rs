use std::path::PathBuf;

use once_cell::sync::OnceCell;

#[derive(Debug)]
pub(crate) struct State {
    pub article_dir: PathBuf,
    pub out_dir: PathBuf,
    pub public_dir: PathBuf,

    pub blog_name: String,

    pub handlebars: handlebars::Handlebars<'static>,
}

pub(super) static STATE: OnceCell<State> = OnceCell::new();

impl State {
    pub fn instance() -> &'static State {
        STATE.get().unwrap()
    }
}
