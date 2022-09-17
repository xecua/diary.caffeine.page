use std::path::PathBuf;

use serde::Serialize;

#[derive(serde::Serialize, Debug, Clone)]
pub(super) struct Metadata {
    pub title: String,
    pub tags: Vec<String>,
    pub date: Option<chrono::NaiveDate>,
    pub path: std::path::PathBuf,
    pub body: String,
}

#[derive(Serialize, Debug)]
pub(super) struct ArticlePageData<'a> {
    pub blog_name: &'static str,
    pub body: String,
    pub meta: &'a Metadata,
}

#[derive(Serialize, Debug)]
pub(super) struct ListPageData<'a> {
    pub blog_name: &'static str,
    pub title: String,
    pub path: PathBuf,
    pub articles: Vec<&'a Metadata>,
}
