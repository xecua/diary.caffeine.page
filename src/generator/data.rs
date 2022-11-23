use serde::Serialize;
use std::{fs::Metadata as FileMetadata, path::PathBuf, rc::Rc};

#[derive(Serialize, Debug, Clone)]
pub(super) struct ArticleMetadata {
    pub title: String,
    pub tags: Vec<String>,
    pub date: Option<chrono::NaiveDate>,
    pub relpath: PathBuf,
    pub is_page: bool,
    pub body: String,

    #[allow(dead_code)]
    #[serde(skip_serializing)]
    pub file_meta: FileMetadata,
}

impl ArticleMetadata {
    pub fn new(file_meta: FileMetadata) -> Self {
        Self {
            title: String::new(),
            tags: Vec::new(),
            date: None,
            relpath: PathBuf::new(),
            is_page: false,
            body: String::new(),
            file_meta,
        }
    }
}

#[derive(Serialize, Debug)]
pub(super) struct ArticlePageData<'a> {
    pub blog_name: &'static str,
    pub body: String,
    pub meta: &'a ArticleMetadata,
}

#[derive(Serialize, Debug)]
pub(super) struct ListPageData {
    pub blog_name: &'static str,
    pub title: String,
    pub relpath: PathBuf,
    pub is_page: bool,
    pub articles: Vec<Rc<ArticleMetadata>>,
}
