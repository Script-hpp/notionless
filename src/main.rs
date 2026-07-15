mod model;

use model::{PaperlessResponse, NotionResponse};
use std::collections::HashMap;


async fn fetch_paperless_memory(
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<HashMap<String, String>,Box<dyn std::error::Error>> {
    let mut paperless_map : HashMap<String, String> = HashMap::new();
    let response = client.get(url)
        .header("Authorization", format!("Token {}", token))
        .send()
        .await?
        .json::<PaperlessResponse>()
        .await?;
    // println!("ROHDATEN AUS PAPERLESS: {:#?}", response);
    for doc in response.results {
        let mut notion_id : Option<String> = None;
        let mut notion_last_edited : Option<String> = None;
        for custom_field in doc.custom_fields {
            if custom_field.field == 1 {
                notion_id = custom_field.value;
            } else if custom_field.field == 2 {
                notion_last_edited = custom_field.value;
            }
        }

        if let (Some(id), Some(edited)) = (notion_id.as_ref(), notion_last_edited.as_ref()) {
                paperless_map.insert(id.clone(), edited.clone());
            }
    }
    
    Ok(paperless_map)
}

async fn fetch_notion_memory(
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut notion_map : HashMap<String, String> = HashMap::new();
    let response = client.post(url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Notion-Version", "2022-06-28")
        .send()
        .await?
        .json::<NotionResponse>()
        .await?;

    
    for page in response.results {
        notion_map.insert(page.id, page.last_edited_time);
    }

    Ok(notion_map)
}

#[tokio::main]
async fn main()-> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    println!("Sync Engine is running!");
    // Paperless Setup
    let client = reqwest::Client::new();
    let url = std::env::var("PAPERLESS_URL").expect("PAPERLESS_URL must be set");
    let token = std::env::var("PAPERLESS_TOKEN").expect("PAPERLESS_TOKEN must be set");

    // Notion Setup
    let notion_url = std::env::var("NOTION_URL").expect("NOTION_URL must be set");
    let notion_token = std::env::var("NOTION_TOKEN").expect("NOTION_TOKEN must be set");
    
    let notion_map = fetch_notion_memory(&client, &notion_url, &notion_token).await?;
    let paperless_map = fetch_paperless_memory(&client, &url, &token).await?;


    println!("Paperless Memory: {:?}", paperless_map);
    println!("Notion Memory: {:?}", notion_map);

    Ok(())
}
