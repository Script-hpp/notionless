use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct PaperlessDocument
{
    pub id: i64,
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

/// Eine in Paperless angelegte Custom-Field-Definition. Die IDs sind pro Instanz
/// verschieden, deshalb wird beim Start über den `name` aufgelöst.
#[derive(Deserialize, Debug)]
pub struct CustomFieldDefinition
{
    pub id: i64,
    pub name: String,
}

#[derive(Deserialize, Debug)]
pub struct CustomFieldListResponse
{
    pub next: Option<String>,
    pub results: Vec<CustomFieldDefinition>,
}

/// Die zur Laufzeit aufgelösten Feld-IDs dieser Paperless-Instanz.
#[derive(Debug, Clone, Copy)]
pub struct FieldIds
{
    pub notion_id: i64,
    pub last_edited: i64,
    pub content_hash: i64,
}

#[derive(Deserialize, Debug)]
pub struct NotionResponse {
    pub results: Vec<NotionPage>,
    #[serde(default)]
    pub has_more: bool,
    #[serde(default)]
    pub next_cursor: Option<String>,
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

#[derive(Deserialize, Debug)]
pub struct NotionBlockResponse {
    pub results: Vec<NotionBlock>,
}

#[derive(Deserialize, Debug)]
pub struct NotionBlock {
    pub r#type: String, 
    pub paragraph: Option<NotionParagraph>,
    pub heading_1: Option<NotionHeading>,
    pub heading_2: Option<NotionHeading>,
    pub heading_3: Option<NotionHeading>,
}

#[derive(Deserialize, Debug)]
pub struct NotionParagraph { pub rich_text: Vec<NotionRichText> }

#[derive(Deserialize, Debug)]
pub struct NotionHeading { pub rich_text: Vec<NotionRichText> }

#[derive(Deserialize, Debug)]
pub struct NotionRichText { pub plain_text: String }