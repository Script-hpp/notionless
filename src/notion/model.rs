use serde::Deserialize;

/// A run of plain text, used both for database titles and block content.
#[derive(Deserialize, Debug, Clone)]
pub struct RichText {
    pub plain_text: String,
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
    pub name: NotionTitleProperty,
}

#[derive(Deserialize, Debug)]
pub struct NotionTitleProperty {
    pub title: Vec<RichText>,
}

#[derive(Deserialize, Debug)]
pub struct NotionBlockResponse {
    pub results: Vec<NotionBlock>,
}

#[derive(Deserialize, Debug)]
pub struct NotionBlock {
    pub r#type: String,
    pub paragraph: Option<NotionRichTextBlock>,
    pub heading_1: Option<NotionRichTextBlock>,
    pub heading_2: Option<NotionRichTextBlock>,
    pub heading_3: Option<NotionRichTextBlock>,
}

#[derive(Deserialize, Debug)]
pub struct NotionRichTextBlock {
    pub rich_text: Vec<RichText>,
}
