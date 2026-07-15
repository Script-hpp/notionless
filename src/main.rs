mod model;

use model::{PaperlessResponse};
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

#[tokio::main]
async fn main()-> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    println!("Sync Engine is running!");
    let client = reqwest::Client::new();
    let url = std::env::var("PAPERLESS_URL").expect("PAPERLESS_URL must be set");
    let token = std::env::var("PAPERLESS_TOKEN").expect("PAPERLESS_TOKEN must be set");

    let paperless_map = fetch_paperless_memory(&client, &url, &token).await?;

    println!("Paperless Memory: {:?}", paperless_map);

    Ok(())
}
