mod model;
mod helpers;

use model::{PaperlessResponse, NotionResponse};
use std::collections::HashMap;


async fn fetch_paperless_memory(
    client: &reqwest::Client,
    start_url: &str,
    token: &str,
) -> Result<HashMap<String, String>,Box<dyn std::error::Error>> {
    let mut paperless_map : HashMap<String, String> = HashMap::new();
    let mut current_url : Option<String> = Some(start_url.to_string());

    while let Some(url) = current_url {
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

            current_url = response.next.map(|mut url| {
                // very secure fix dont try it at home
                if url.starts_with("http://") {
                    url = url.replace("http://", "https://");
                }

                if let Some(query_idx) = url.find('?') {
                    let base = &url[..query_idx];
                    if !base.ends_with('/') {
                        url.insert(query_idx, '/');
                    }
                } else if !url.ends_with('/') {
                    url.push('/');
                }
                url
        });    
    }
    
    Ok(paperless_map)
}

async fn fetch_notion_memory(
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<HashMap<String, (String, String)>, Box<dyn std::error::Error>> {
    let mut notion_map : HashMap<String, (String, String)> = HashMap::new();
    let response = client.post(url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Notion-Version", "2022-06-28")
        .send()
        .await?
        .json::<NotionResponse>()
        .await?;

    for page in response.results {
        // Wir holen den inneren String aus dem NotionText-Struct
        let title = page.properties.name.title
            .first()
            .map(|t| t.plain_text.clone()) // <-- Falls das Feld im Struct "plain_text" heißt, ändere es hier zu t.plain_text.clone()
            .unwrap_or_else(|| "Untitled".to_string());

        notion_map.insert(page.id, (page.last_edited_time, title));    
    }

    Ok(notion_map)
}

#[tokio::main]
async fn main()-> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    println!("Sync Engine is running!");
    // Paperless Setup
    let client = reqwest::Client::new();
    let paperless_url = std::env::var("PAPERLESS_URL").expect("PAPERLESS_URL must be set");
    let paperless_token = std::env::var("PAPERLESS_TOKEN").expect("PAPERLESS_TOKEN must be set");

    // Notion Setup
    let notion_url = std::env::var("NOTION_URL").expect("NOTION_URL must be set");
    let notion_token = std::env::var("NOTION_TOKEN").expect("NOTION_TOKEN must be set");
    
    let notion_map = fetch_notion_memory(&client, &notion_url, &notion_token).await?;
    let paperless_map = fetch_paperless_memory(&client, &paperless_url, &paperless_token).await?;


    println!("Paperless Memory: {:?}", paperless_map);
    println!("Notion Memory: {:?}", notion_map);

    // 3. Abgleich berechnen
    println!("\n--- Berechne Synchronisation ---");
    let sync_actions = helpers::compare_memories(&paperless_map, &notion_map);

    // 4. Ergebnisse ausgeben
    for (notion_id, action) in &sync_actions {
        match action {
            model::SyncAction::CreateInPaperless => {
                println!("➔ [NOTION-ID: {}]: Muss in Paperless erstellt werden.", notion_id);
            }
            model::SyncAction::UpdateNotion => {
                println!("➔ [NOTION-ID: {}]: Notion-Eintrag veraltet. Update Notion!", notion_id);
            }
            model::SyncAction::UpdatePaperless => {
                println!("➔ [NOTION-ID: {}]: Paperless-Eintrag veraltet. Update Paperless!", notion_id);
            }
            model::SyncAction::UpToDate => {
                println!("➔ [NOTION-ID: {}]: Bereits auf dem neuesten Stand.", notion_id);
            }
        }
    }

    Ok(())
}
