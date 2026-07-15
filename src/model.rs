use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct PaperlessDocument
{
    pub custom_fields: Vec<CustomFields>,
}

#[derive(Deserialize, Debug)]
pub struct PaperlessResponse
{
    pub results: Vec<PaperlessDocument>,
}

#[derive(Deserialize, Debug)]
pub struct CustomFields
{
    pub field: i64,
    pub value: Option<String>,
}