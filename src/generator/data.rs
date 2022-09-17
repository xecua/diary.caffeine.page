use serde::Serialize;
use std::{fs::Metadata as FileMetadata, path::PathBuf};

#[derive(Serialize, Debug, Clone)]
pub(super) struct ArticleMetadata {
    pub title: String,
    pub tags: Vec<String>,
    pub date: Option<chrono::NaiveDate>,
    pub path: std::path::PathBuf,
    pub body: String,

    #[serde(skip_serializing)]
    pub file_meta: FileMetadata,
}

#[derive(Serialize, Debug)]
pub(super) struct ArticlePageData<'a> {
    pub blog_name: &'static str,
    pub body: String,
    pub meta: &'a ArticleMetadata,
}

#[derive(Serialize, Debug)]
pub(super) struct ListPageData<'a> {
    pub blog_name: &'static str,
    pub title: String,
    pub path: PathBuf,
    pub articles: Vec<&'a ArticleMetadata>,
}
