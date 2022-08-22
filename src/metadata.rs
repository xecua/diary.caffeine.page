#[derive(serde::Serialize, Debug)]
pub(crate) struct Metadata {
    pub title: String,
    pub tags: Vec<String>,
    pub date: chrono::NaiveDate, // 投稿日: どうすっかな(やるならchronoを追加する)
    pub path: std::path::PathBuf,
    pub body: String,
}
