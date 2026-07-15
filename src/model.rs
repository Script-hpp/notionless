use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct PaperlessDocument
{
    pub custom_fields: Vec<CustomFields>,
}

#[derive(Deserialize, Debug)]
pub struct PaperlessResponse
{
    pub next : Option<String>,
    pub results: Vec<PaperlessDocument>,
}

#[derive(Deserialize, Debug)]
pub struct CustomFields
{
    pub field: i64,
    pub value: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct NotionResponse {
    pub results: Vec<NotionPage>,
}

#[derive(Deserialize, Debug)]
pub struct NotionPage {
    pub id: String,
    pub last_edited_time: String,
    pub properties: NotionProperties,
}

#[derive(Deserialize, Debug)]
pub struct NotionProperties {
    #[serde(rename = "Name")]
    pub name: NotionNameProperty,
}

#[derive(Deserialize, Debug)]
pub struct NotionNameProperty {
    pub title: Vec<NotionTitleText>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct NotionTitleText {
    pub plain_text: String,
}

#[derive(Debug)]
pub enum SyncAction {
    UpdateNotion,       // Paperless ist neuer
    UpdatePaperless,    // Notion ist neuer
    CreateInPaperless,  // Existiert nur in Notion
    UpToDate,           // Beide sind auf dem gleichen Stand
}