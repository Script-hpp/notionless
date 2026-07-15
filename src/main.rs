mod model;
mod helpers;

use model::{PaperlessResponse, NotionResponse};
use std::collections::HashMap;
use reqwest::multipart;

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

async fn export_notion_page_content(
    client: &reqwest::Client,
    page_id: &str,
    token: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let url = format!("https://api.notion.com/v1/blocks/{}/children", page_id);
    let response = client.get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Notion-Version", "2022-06-28")
        .send().await?.json::<model::NotionBlockResponse>().await?;

    let mut markdown = String::new();
    for block in response.results {
        match block.r#type.as_str() {
            "paragraph" => if let Some(p) = block.paragraph {
                let text: String = p.rich_text.iter().map(|t| t.plain_text.as_str()).collect();
                markdown.push_str(&format!("{}\n\n", text));
            },
            "heading_1" => if let Some(h) = block.heading_1 {
                let text: String = h.rich_text.iter().map(|t| t.plain_text.as_str()).collect();
                markdown.push_str(&format!("# {}\n\n", text));
            },
            "heading_2" => if let Some(h) = block.heading_2 {
                let text: String = h.rich_text.iter().map(|t| t.plain_text.as_str()).collect();
                markdown.push_str(&format!("## {}\n\n", text));
            },
            "heading_3" => if let Some(h) = block.heading_3 {
                let text: String = h.rich_text.iter().map(|t| t.plain_text.as_str()).collect();
                markdown.push_str(&format!("### {}\n\n", text));
            },
            _ => {} // Andere Blöcke ignorieren wir fürs Erste
        }
    }
    Ok(markdown)
}

async fn upload_to_paperless(
    client: &reqwest::Client,
    paperless_url: &str,
    token: &str,
    notion_id: &str,
    title: &str,
    markdown_content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Sicherheit: Leerzeichen/Zeilenumbrüche aus .env-Variablen strippen
    let clean_url = paperless_url.trim().trim_end_matches('/');
    let clean_token = token.trim();
    
    // Wir erzwingen HTTPS, falls in der .env noch http:// steht
    let secure_url = if clean_url.starts_with("http://") {
        clean_url.replace("http://", "https://")
    } else {
        clean_url.to_string()
    };

    let base_domain = secure_url.split("/api").next().unwrap_or(&secure_url);

    let upload_url = format!("{}/api/documents/post_document/", base_domain);

    // 2. Datei-Part und Metadaten bauen
    let file_part = multipart::Part::bytes(markdown_content.to_string().into_bytes())
        .file_name(format!("{}.md", title))
        .mime_str("text/markdown")?;

    let custom_fields_json = format!("{{\"1\": \"{}\"}}", notion_id);
    
    let form = multipart::Form::new()
        .part("document", file_part)
        .text("title", title.to_string())
        .text("custom_fields", custom_fields_json);

    // 3. Request mit Django-CSRF-Panzerung abschicken
    let response = client.post(&upload_url)
        .header("Authorization", format!("Token {}", clean_token))
        .header("Accept", "application/json")
        .header("Referer", &upload_url)
        .header("Origin", &secure_url)
        .multipart(form)
        .send().await?;

    // 4. Ergebnis auswerten
    if response.status().is_success() {
        println!("  ✓ Erfolgreich in Paperless hochgeladen: {}", title);
    } else {
        let status = response.status();
        let error_text = response.text().await?;
        println!("  ✗ Fehler beim Upload von '{}' (Status: {}):", title, status);
        println!("    Grund: {}", error_text);
        println!("    URL war: {}", upload_url);
    }
    
    Ok(())
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
                if let Some((_, title)) = notion_map.get(notion_id) {
                    match export_notion_page_content(&client, notion_id, &notion_token).await {
                        Ok(markdown) => {
                            let _ = upload_to_paperless(&client, &paperless_url, &paperless_token, notion_id, title, &markdown).await;
                        }
                        Err(e) => println!("  ✗ Fehler beim Export aus Notion: {}", e),
                    }
                }
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
