#[derive(serde::Serialize, Debug, Clone)]
pub(crate) struct Metadata {
    pub title: String,
    pub tags: Vec<String>,
    pub date: Option<chrono::NaiveDate>,
    pub path: std::path::PathBuf,
    pub body: String,
}
