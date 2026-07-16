use serde::Deserialize;

/// A single custom-field value attached to a Paperless document.
#[derive(Deserialize, Debug)]
pub struct CustomFieldValue {
    pub field: i64,
    pub value: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct PaperlessDocument {
    pub id: i64,
    pub custom_fields: Vec<CustomFieldValue>,
}

#[derive(Deserialize, Debug)]
pub struct PaperlessResponse {
    pub next: Option<String>,
    pub results: Vec<PaperlessDocument>,
}

/// A custom-field's schema definition, as opposed to its value on one document.
#[derive(Deserialize, Debug)]
pub struct CustomFieldDefinition {
    pub id: i64,
    pub name: String,
}

#[derive(Deserialize, Debug)]
pub struct CustomFieldListResponse {
    pub next: Option<String>,
    pub results: Vec<CustomFieldDefinition>,
}

/// The custom-field IDs resolved for this Paperless instance at startup.
#[derive(Debug, Clone, Copy)]
pub struct CustomFieldIds {
    pub notion_id: i64,
    pub last_edited: i64,
    pub content_hash: i64,
}
