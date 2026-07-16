pub mod model;

use model::{ NotionBlockResponse, NotionResponse };
use std::collections::HashMap;

/// Metadata about a Notion page as returned by the database query, before its content
/// has been exported.
pub struct PageSummary {
    pub last_edited_time: String,
    pub title: String,
}

/// Fetches id -> metadata for every page in the configured Notion database.
/// Notion paginates at 100 results per page; keep querying until `has_more` is false.
pub async fn fetch_memory(
    client: &reqwest::Client,
    url: &str,
    token: &str
) -> Result<HashMap<String, PageSummary>, Box<dyn std::error::Error>> {
    let mut pages: HashMap<String, PageSummary> = HashMap::new();
    let mut start_cursor: Option<String> = None;

    loop {
        let mut body = serde_json::json!({ "page_size": 100 });
        if let Some(cursor) = &start_cursor {
            body["start_cursor"] = serde_json::json!(cursor);
        }

        let response = client
            .post(url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Notion-Version", "2022-06-28")
            .json(&body)
            .send().await?
            .json::<NotionResponse>().await?;

        for page in response.results {
            let title = page.properties.name.title
                .first()
                .map(|t| t.plain_text.clone())
                .unwrap_or_else(|| "Untitled".to_string());

            pages.insert(page.id, PageSummary { last_edited_time: page.last_edited_time, title });
        }

        if !response.has_more {
            break;
        }
        match response.next_cursor {
            Some(cursor) => start_cursor = Some(cursor),
            None => break,
        }
    }

    Ok(pages)
}

/// Exports a single Notion page's content as Markdown.
///
/// Only paragraphs and headings 1-3 are currently supported; other block types
/// (lists, code, to-dos, tables, nested blocks) are silently skipped.
pub async fn export_page_content(
    client: &reqwest::Client,
    page_id: &str,
    token: &str
) -> Result<String, Box<dyn std::error::Error>> {
    let url = format!("https://api.notion.com/v1/blocks/{}/children", page_id);
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Notion-Version", "2022-06-28")
        .send().await?
        .json::<NotionBlockResponse>().await?;

    let mut markdown = String::new();
    for block in response.results {
        let (prefix, text_block) = match block.r#type.as_str() {
            "paragraph" => ("", block.paragraph),
            "heading_1" => ("# ", block.heading_1),
            "heading_2" => ("## ", block.heading_2),
            "heading_3" => ("### ", block.heading_3),
            _ => continue,
        };

        if let Some(text_block) = text_block {
            let text: String = text_block.rich_text.iter().map(|t| t.plain_text.as_str()).collect();
            markdown.push_str(&format!("{}{}\n\n", prefix, text));
        }
    }
    Ok(markdown)
}
