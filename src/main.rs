mod model;
mod helpers;

use model::{PaperlessResponse, NotionResponse};
use std::collections::HashMap;
use reqwest::multipart;

async fn fetch_paperless_memory(
    client: &reqwest::Client,
    start_url: &str,
    token: &str,
) -> Result<HashMap<String, (i64,String)>,Box<dyn std::error::Error>> {
    let mut paperless_map : HashMap<String, (i64, String)> = HashMap::new();
    let mut current_url : Option<String> = Some(start_url.to_string());

    while let Some(url) = current_url {
        let response = client.get(url)
            .header("Authorization", format!("Token {}", token))
            .send()
            .await?
            .json::<PaperlessResponse>()
            .await?;
        for doc in response.results {
            let mut notion_id : Option<String> = None;
            let mut notion_last_edited : Option<String> = None;
            for custom_field in doc.custom_fields {
                if custom_field.field == 1 {
                    notion_id = custom_field.value;
                } else if custom_field.field == 4 {
                    notion_last_edited = custom_field.value;
                }
            }

            if let (Some(id), Some(edited)) = (notion_id.as_ref(), notion_last_edited.as_ref()) {
                    paperless_map.insert(id.clone(), (doc.id, edited.clone()));
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
            .map(|t| t.plain_text.clone())
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
            _ => {}
        }
    }
    Ok(markdown)
}

async fn delete_from_paperless(
    client: &reqwest::Client,
    paperless_url: &str,
    token: &str,
    document_id: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let clean_url = paperless_url.trim().trim_end_matches('/');
    let clean_token = token.trim();
    
    let secure_url = if clean_url.starts_with("http://") {
        clean_url.replace("http://", "https://")
    } else {
        clean_url.to_string()
    };

    let base_domain = secure_url.split("/api").next().unwrap_or(&secure_url);
    let delete_url = format!("{}/api/documents/{}/", base_domain, document_id);

    let response = client.delete(&delete_url)
        .header("Authorization", format!("Token {}", clean_token))
        .header("Accept", "application/json")
        .header("Referer", &delete_url)
        .header("Origin", &secure_url)
        .send().await?;

    if response.status().is_success() || response.status() == 204 {
        println!("  ✓ Altes Dokument (ID: {}) erfolgreich aus Paperless gelöscht.", document_id);
    } else {
        println!("  ✗ Fehler beim Löschen von ID {} (Status: {}):", document_id, response.status());
        if let Ok(text) = response.text().await {
            println!("    Grund: {}", text);
        }
    }
    
    Ok(())
}

async fn upload_to_paperless(
    client: &reqwest::Client,
    paperless_url: &str,
    token: &str,
    notion_id: &str,
    last_edited_time: &str,
    title: &str,
    markdown_content: &str,
) -> Result<(), Box<dyn std::error::Error>> {

    let clean_url = paperless_url.trim().trim_end_matches('/');
    let clean_token = token.trim();
    
    let secure_url = if clean_url.starts_with("http://") {
        clean_url.replace("http://", "https://")
    } else {
        clean_url.to_string()
    };

    let base_domain = secure_url.split("/api").next().unwrap_or(&secure_url);

    let upload_url = format!("{}/api/documents/post_document/", base_domain);

    let file_part = multipart::Part::bytes(markdown_content.to_string().into_bytes())
        .file_name(format!("{}.md", title))
        .mime_str("text/markdown")?;

    // this needs proper documentation
    let custom_fields_json = format!(
            "{{\"1\": \"{}\", \"4\": \"{}\"}}", 
            notion_id, last_edited_time
        );
    let form = multipart::Form::new()
        .part("document", file_part)
        .text("title", title.to_string())
        .text("custom_fields", custom_fields_json);

    let response = client.post(&upload_url)
        .header("Authorization", format!("Token {}", clean_token))
        .header("Accept", "application/json")
        .header("Referer", &upload_url)
        .header("Origin", &secure_url)
        .multipart(form)
        .send().await?;

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
                if let Some((last_edited_time, title)) = notion_map.get(notion_id) {
                    match export_notion_page_content(&client, notion_id, &notion_token).await {
                        Ok(markdown) => {
                            let _ = upload_to_paperless(&client, &paperless_url, &paperless_token, notion_id, last_edited_time, title, &markdown).await;
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
                if let Some((paperless_id, _)) = paperless_map.get(notion_id) {
                    if let Some((last_edited_time, title)) = notion_map.get(notion_id) {
                        println!("  - Lösche alte Version (Paperless ID: {})...", paperless_id);
                        let _ = delete_from_paperless(&client, &paperless_url, &paperless_token, *paperless_id).await;
                        match export_notion_page_content(&client, notion_id, &notion_token).await {
                            Ok(markdown) => {
                                let _ = upload_to_paperless(
                                    &client, &paperless_url, &paperless_token, 
                                    notion_id, last_edited_time, title, &markdown
                                ).await;
                            }
                            Err(e) => println!("  ✗ Fehler beim Export aus Notion: {}", e),
                        }
                    }
                }
            }
            model::SyncAction::UpToDate => {
                println!("➔ [NOTION-ID: {}]: Bereits auf dem neuesten Stand.", notion_id);
            }
        }
    }

    Ok(())
}
