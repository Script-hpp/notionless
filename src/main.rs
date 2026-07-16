mod model;
mod helpers;

use model::{ PaperlessResponse, NotionResponse, FieldIds };
use std::time::Duration;
use tokio::time::sleep;
use std::collections::HashMap;
use reqwest::multipart;

// Namen der Custom-Fields in Paperless. Die numerischen IDs unterscheiden sich pro
// Instanz und werden beim Start über diese Namen aufgelöst (siehe `ensure_custom_fields`).
const FIELD_NAME_NOTION_ID: &str = "notion_id";
const FIELD_NAME_LAST_EDITED: &str = "notion_last_edited";
const FIELD_NAME_CONTENT_HASH: &str = "notion_content_hash";

/// Schneidet einen Pfad-Anteil (z. B. `/api/documents/`) von der konfigurierten URL ab,
/// sodass die Basis der Paperless-Instanz übrig bleibt. Schema und Port bleiben unangetastet.
fn base_domain(paperless_url: &str) -> String {
    let clean = paperless_url.trim().trim_end_matches('/');
    clean.split("/api").next().unwrap_or(clean).trim_end_matches('/').to_string()
}

/// Baut die `next`-URL der Paperless-Pagination auf die konfigurierte Basis um.
///
/// Paperless bildet `next` aus dem, was es für seinen eigenen Host hält. Hinter einem
/// Reverse-Proxy ohne `X-Forwarded-Proto` ist das `http://`, obwohl die Instanz unter
/// `https://` läuft. Folgt man dem, verwirft reqwest beim Redirect auf https den
/// Authorization-Header (Schema-Wechsel = fremder Origin) und Paperless antwortet 401.
///
/// Deshalb übernehmen wir nur Pfad + Query und hängen sie an die konfigurierte Basis:
/// eine LAN-Instanz bleibt so http, eine Proxy-Instanz https.
fn normalize_next_url(next: &str, base: &str) -> String {
    let path_and_query = match next.find("://") {
        Some(scheme_end) => {
            let after_scheme = &next[scheme_end + 3..];
            match after_scheme.find('/') {
                Some(path_start) => &after_scheme[path_start..],
                None => "/",
            }
        }
        None => next,
    };

    let mut url = format!("{}{}", base, path_and_query);

    // Manche Setups liefern den Pfad ohne abschließenden Slash vor dem Query-String.
    if let Some(query_idx) = url.find('?') {
        if !url[..query_idx].ends_with('/') {
            url.insert(query_idx, '/');
        }
    } else if !url.ends_with('/') {
        url.push('/');
    }

    url
}

/// Löst die Custom-Field-IDs dieser Paperless-Instanz über ihre Namen auf und legt
/// fehlende Felder an. Damit läuft der Sync ohne manuelle Konfiguration gegen eine
/// beliebige Instanz.
async fn ensure_custom_fields(
    client: &reqwest::Client,
    base: &str,
    token: &str
) -> Result<FieldIds, Box<dyn std::error::Error>> {
    let mut existing: HashMap<String, i64> = HashMap::new();
    let mut current_url: Option<String> = Some(format!("{}/api/custom_fields/?page_size=100", base));

    while let Some(url) = current_url {
        let response = client
            .get(&url)
            .header("Authorization", format!("Token {}", token))
            .header("Accept", "application/json")
            .send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(
                format!("Custom-Fields konnten nicht gelesen werden (Status {}): {}", status, text).into()
            );
        }

        let page = response.json::<model::CustomFieldListResponse>().await?;
        for field in page.results {
            existing.insert(field.name, field.id);
        }
        current_url = page.next.map(|next| normalize_next_url(&next, base));
    }

    Ok(FieldIds {
        notion_id: get_or_create_field(client, base, token, &existing, FIELD_NAME_NOTION_ID).await?,
        last_edited: get_or_create_field(client, base, token, &existing, FIELD_NAME_LAST_EDITED).await?,
        content_hash: get_or_create_field(client, base, token, &existing, FIELD_NAME_CONTENT_HASH).await?,
    })
}

async fn get_or_create_field(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    existing: &HashMap<String, i64>,
    name: &str
) -> Result<i64, Box<dyn std::error::Error>> {
    if let Some(id) = existing.get(name) {
        println!("  ✓ Custom-Field '{}' gefunden (ID {}).", name, id);
        return Ok(*id);
    }

    let create_url = format!("{}/api/custom_fields/", base);
    let body = serde_json::json!({ "name": name, "data_type": "string" });
    let response = client
        .post(&create_url)
        .header("Authorization", format!("Token {}", token))
        .header("Accept", "application/json")
        .header("Referer", &create_url)
        .header("Origin", base)
        .json(&body)
        .send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(
            format!("Custom-Field '{}' konnte nicht angelegt werden (Status {}): {}", name, status, text).into()
        );
    }

    let field = response.json::<model::CustomFieldDefinition>().await?;
    println!("  + Custom-Field '{}' in Paperless angelegt (ID {}).", name, field.id);
    Ok(field.id)
}

async fn fetch_paperless_memory(
    client: &reqwest::Client,
    start_url: &str,
    token: &str,
    fields: &FieldIds
) -> Result<HashMap<String, (i64, String, String)>, Box<dyn std::error::Error>> {
    // notion_id -> (paperless_id, last_edited_time, content_hash)
    let mut paperless_map: HashMap<String, (i64, String, String)> = HashMap::new();
    let base = base_domain(start_url);
    let mut current_url: Option<String> = Some(start_url.to_string());

    while let Some(url) = current_url {
        let response = client
            .get(url)
            .header("Authorization", format!("Token {}", token))
            .send().await?
            .json::<PaperlessResponse>().await?;
        for doc in response.results {
            let mut notion_id: Option<String> = None;
            let mut notion_last_edited: Option<String> = None;
            let mut content_hash: Option<String> = None;
            for custom_field in doc.custom_fields {
                if custom_field.field == fields.notion_id {
                    notion_id = custom_field.value;
                } else if custom_field.field == fields.last_edited {
                    notion_last_edited = custom_field.value;
                } else if custom_field.field == fields.content_hash {
                    content_hash = custom_field.value;
                }
            }

            if let (Some(id), Some(edited)) = (notion_id.as_ref(), notion_last_edited.as_ref()) {
                let hash = content_hash.unwrap_or_default();
                paperless_map.insert(id.clone(), (doc.id, edited.clone(), hash));
            }
        }

        current_url = response.next.map(|next| normalize_next_url(&next, &base));
    }

    Ok(paperless_map)
}

async fn fetch_notion_memory(
    client: &reqwest::Client,
    url: &str,
    token: &str
) -> Result<HashMap<String, (String, String)>, Box<dyn std::error::Error>> {
    let mut notion_map: HashMap<String, (String, String)> = HashMap::new();
    let mut start_cursor: Option<String> = None;

    // Notion liefert max. 100 Ergebnisse pro Seite -> so lange abfragen,
    // bis has_more == false ist.
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
            // Wir holen den inneren String aus dem NotionText-Struct
            let title = page.properties.name.title
                .first()
                .map(|t| t.plain_text.clone())
                .unwrap_or_else(|| "Untitled".to_string());

            notion_map.insert(page.id, (page.last_edited_time, title));
        }

        if response.has_more {
            match response.next_cursor {
                Some(cursor) => start_cursor = Some(cursor),
                None => break,
            }
        } else {
            break;
        }
    }

    Ok(notion_map)
}

async fn export_notion_page_content(
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
        .json::<model::NotionBlockResponse>().await?;

    let mut markdown = String::new();
    for block in response.results {
        match block.r#type.as_str() {
            "paragraph" => if let Some(p) = block.paragraph {
                let text: String = p.rich_text
                    .iter()
                    .map(|t| t.plain_text.as_str())
                    .collect();
                markdown.push_str(&format!("{}\n\n", text));
            }
            "heading_1" => if let Some(h) = block.heading_1 {
                let text: String = h.rich_text
                    .iter()
                    .map(|t| t.plain_text.as_str())
                    .collect();
                markdown.push_str(&format!("# {}\n\n", text));
            }
            "heading_2" => if let Some(h) = block.heading_2 {
                let text: String = h.rich_text
                    .iter()
                    .map(|t| t.plain_text.as_str())
                    .collect();
                markdown.push_str(&format!("## {}\n\n", text));
            }
            "heading_3" => if let Some(h) = block.heading_3 {
                let text: String = h.rich_text
                    .iter()
                    .map(|t| t.plain_text.as_str())
                    .collect();
                markdown.push_str(&format!("### {}\n\n", text));
            }
            _ => {}
        }
    }
    Ok(markdown)
}

async fn delete_from_paperless(
    client: &reqwest::Client,
    paperless_url: &str,
    token: &str,
    document_id: i64
) -> Result<(), Box<dyn std::error::Error>> {
    let clean_token = token.trim();
    let base_domain = base_domain(paperless_url);
    let delete_url = format!("{}/api/documents/{}/", base_domain, document_id);

    let response = client
        .delete(&delete_url)
        .header("Authorization", format!("Token {}", clean_token))
        .header("Accept", "application/json")
        .header("Referer", &delete_url)
        .header("Origin", &base_domain)
        .send().await?;

    if response.status().is_success() || response.status() == 204 {
        println!("  ✓ Altes Dokument (ID: {}) in den Papierkorb verschoben.", document_id);

        // WICHTIG: DELETE verschiebt nur in den Papierkorb. Paperless' Duplikat-Prüfung
        // durchsucht den Papierkorb mit -> ein Re-Upload würde sonst als
        // "It is a duplicate ... existing document is in the trash" abgelehnt.
        // Deshalb endgültig aus dem Papierkorb entfernen.
        let trash_url = format!("{}/api/trash/", base_domain);
        let trash_body = serde_json::json!({ "action": "empty", "documents": [document_id] });
        let trash_response = client
            .post(&trash_url)
            .header("Authorization", format!("Token {}", clean_token))
            .header("Accept", "application/json")
            .json(&trash_body)
            .send().await?;

        if trash_response.status().is_success() {
            println!("  ✓ Dokument (ID: {}) endgültig aus dem Papierkorb entfernt.", document_id);
        } else {
            let status = trash_response.status();
            println!("  ⚠ Konnte Papierkorb für ID {} nicht leeren (Status: {}). Re-Upload könnte als Duplikat scheitern.", document_id, status);
            if let Ok(text) = trash_response.text().await {
                println!("    Grund: {}", text);
            }
        }
    } else {
        println!("  ✗ Fehler beim Löschen von ID {} (Status: {}):", document_id, response.status());
        if let Ok(text) = response.text().await {
            println!("    Grund: {}", text);
        }
    }

    Ok(())
}

/// Ergebnis eines abgewarteten Paperless-Tasks.
enum TaskResult {
    Success,
    /// Task ist fehlgeschlagen; enthält Paperless' Klartext-Grund (z. B. Duplikat-Meldung).
    Failure(String),
    Timeout,
}

/// Paperless verarbeitet Uploads asynchron. `post_document` liefert nur eine Task-ID;
/// ob das Dokument wirklich aufgenommen wurde (oder z. B. als Duplikat abgelehnt), steht
/// erst danach im Task-Status. Wir pollen deshalb und loggen das echte Ergebnis IMMER.
async fn wait_for_task(
    client: &reqwest::Client,
    base_domain: &str,
    token: &str,
    task_id: &str,
    title: &str
) -> Result<TaskResult, Box<dyn std::error::Error>> {
    let tasks_url = format!("{}/api/tasks/?task_id={}", base_domain, task_id);

    for _ in 0..10 {
        let tasks: Vec<serde_json::Value> = client
            .get(&tasks_url)
            .header("Authorization", format!("Token {}", token))
            .header("Accept", "application/json")
            .send().await?
            .json().await?;

        if let Some(task) = tasks.first() {
            let status = task.get("status").and_then(|s| s.as_str()).unwrap_or("UNKNOWN");
            match status {
                "SUCCESS" => {
                    println!("  ✓ Paperless hat '{}' erfolgreich aufgenommen.", title);
                    return Ok(TaskResult::Success);
                }
                "FAILURE" => {
                    let result = task
                        .get("result")
                        .and_then(|r| r.as_str())
                        .unwrap_or("(kein Grund angegeben)")
                        .to_string();
                    println!("  ✗ Paperless hat '{}' ABGELEHNT (Task {}): {}", title, task_id, result);
                    return Ok(TaskResult::Failure(result));
                }
                _ => {} // PENDING / STARTED / RECEIVED -> weiter warten
            }
        }

        sleep(Duration::from_secs(2)).await;
    }

    println!("  ⚠ Task {} für '{}' nicht rechtzeitig abgeschlossen (Timeout beim Warten).", task_id, title);
    Ok(TaskResult::Timeout)
}

/// Zieht die Paperless-Dokument-ID aus einer Duplikat-Fehlermeldung, z. B.
/// "It is a duplicate of moin moin (#327)." -> Some(327).
fn parse_duplicate_document_id(message: &str) -> Option<i64> {
    let start = message.rfind("(#")? + 2;
    let end = start + message[start..].find(')')?;
    message[start..end].trim().parse().ok()
}

/// Verknüpft ein bereits in Paperless vorhandenes Dokument nachträglich mit einer
/// Notion-Seite. Greift, wenn `post_document` das Dokument als Duplikat ablehnt, weil
/// es (z. B. aus einer Zeit vor notionless oder nach einer Feld-Migration) schon ohne
/// Notion-Zuordnung existiert. Ohne das würde der Sync es bei jedem Durchlauf erneut
/// hochladen und erneut als Duplikat abgelehnt bekommen — eine Endlosschleife.
///
/// Bestehende Custom-Fields werden gelesen und übernommen, statt überschrieben: Paperless'
/// PATCH ersetzt die komplette custom_fields-Liste, ein reines `{"custom_fields": [...]}`
/// mit nur unseren drei Feldern würde sonst andere, manuell gepflegte Felder löschen.
async fn adopt_existing_document(
    client: &reqwest::Client,
    base_domain: &str,
    token: &str,
    document_id: i64,
    fields: &FieldIds,
    notion_id: &str,
    last_edited_time: &str,
    content_hash: &str
) -> Result<(), Box<dyn std::error::Error>> {
    let document_url = format!("{}/api/documents/{}/", base_domain, document_id);

    let existing = client
        .get(&document_url)
        .header("Authorization", format!("Token {}", token))
        .header("Accept", "application/json")
        .send().await?
        .json::<model::PaperlessDocument>().await?;

    let our_field_ids = [fields.notion_id, fields.last_edited, fields.content_hash];
    let mut merged: Vec<serde_json::Value> = existing.custom_fields
        .into_iter()
        .filter(|cf| !our_field_ids.contains(&cf.field))
        .map(|cf| serde_json::json!({ "field": cf.field, "value": cf.value }))
        .collect();
    merged.push(serde_json::json!({ "field": fields.notion_id, "value": notion_id }));
    merged.push(serde_json::json!({ "field": fields.last_edited, "value": last_edited_time }));
    merged.push(serde_json::json!({ "field": fields.content_hash, "value": content_hash }));

    let response = client
        .patch(&document_url)
        .header("Authorization", format!("Token {}", token))
        .header("Accept", "application/json")
        .header("Referer", &document_url)
        .header("Origin", base_domain)
        .json(&serde_json::json!({ "custom_fields": merged }))
        .send().await?;

    if response.status().is_success() {
        println!("  ✓ Vorhandenes Dokument (ID: {}) mit Notion-Seite verknüpft.", document_id);
    } else {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        println!("  ✗ Verknüpfung von Dokument {} fehlgeschlagen (Status {}): {}", document_id, status, text);
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
    content_hash: &str,
    markdown_content: &str,
    fields: &FieldIds
) -> Result<(), Box<dyn std::error::Error>> {
    let clean_token = token.trim();
    let base_domain = base_domain(paperless_url);
    let upload_url = format!("{}/api/documents/post_document/", base_domain);

    let file_part = multipart::Part
        ::bytes(markdown_content.to_string().into_bytes())
        .file_name(format!("{}.md", title))
        .mime_str("text/markdown")?;

    // Die Feld-IDs stammen aus der Discovery beim Start und gelten für diese Instanz.
    let custom_fields_json = serde_json::json!({
        fields.notion_id.to_string(): notion_id,
        fields.last_edited.to_string(): last_edited_time,
        fields.content_hash.to_string(): content_hash,
    }).to_string();
    let form = multipart::Form
        ::new()
        .part("document", file_part)
        .text("title", title.to_string())
        .text("custom_fields", custom_fields_json);

    let response = client
        .post(&upload_url)
        .header("Authorization", format!("Token {}", clean_token))
        .header("Accept", "application/json")
        .header("Referer", &upload_url)
        .header("Origin", &base_domain)
        .multipart(form)
        .send().await?;

    if response.status().is_success() {
        // Body ist die Task-ID (JSON-String). Nur "angenommen" != "aufgenommen":
        // die echte Duplikat-/Fehlerinfo kommt erst aus dem Task-Status.
        let task_id: String = response.json().await.unwrap_or_default();
        let task_id = task_id.trim().to_string();
        if task_id.is_empty() {
            println!("  ⚠ Upload von '{}' angenommen, aber keine Task-ID erhalten.", title);
        } else {
            println!("  … Upload von '{}' angenommen (Task {}). Warte auf Verarbeitung…", title, task_id);
            if let Ok(TaskResult::Failure(message)) = wait_for_task(client, &base_domain, clean_token, &task_id, title).await {
                if let Some(document_id) = parse_duplicate_document_id(&message) {
                    println!("  ↻ '{}' liegt inhaltsgleich bereits in Paperless (ID: {}). Verknüpfe...", title, document_id);
                    let _ = adopt_existing_document(
                        client,
                        &base_domain,
                        clean_token,
                        document_id,
                        fields,
                        notion_id,
                        last_edited_time,
                        content_hash
                    ).await;
                }
            }
        }
    } else {
        let status = response.status();
        let error_text = response.text().await?;
        println!("  ✗ Fehler beim Upload von '{}' (Status: {}):", title, status);
        println!("    Grund: {}", error_text);
        println!("    URL war: {}", upload_url);
    }

    Ok(())
}

/// Führt genau EINEN Synchronisations-Durchlauf aus.
/// Gibt einen Fehler zurück, statt das Programm zu beenden – die Hauptschleife
/// fängt ihn ab und läuft beim nächsten Intervall weiter.
async fn run_sync_cycle(
    client: &reqwest::Client,
    paperless_url: &str,
    paperless_token: &str,
    notion_url: &str,
    notion_token: &str,
    fields: &FieldIds
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. WICHTIG: Daten in JEDEM Durchlauf neu abfragen!
    let notion_map = fetch_notion_memory(client, notion_url, notion_token).await?;
    let paperless_map = fetch_paperless_memory(client, paperless_url, paperless_token, fields).await?;

    // 2. Inhalte EINMAL exportieren, Markdown cachen und Hashes berechnen.
    //    Der Hash ist die maßgebliche Änderungserkennung (nicht der Zeitstempel).
    let mut notion_content: HashMap<String, String> = HashMap::new();
    // notion_id -> (last_edited_time, title, content_hash)
    let mut notion_full: HashMap<String, (String, String, String)> = HashMap::new();
    for (notion_id, (last_edited_time, title)) in &notion_map {
        match export_notion_page_content(client, notion_id, notion_token).await {
            Ok(markdown) => {
                let hash = helpers::compute_content_hash(&markdown);
                notion_full.insert(
                    notion_id.clone(),
                    (last_edited_time.clone(), title.clone(), hash),
                );
                notion_content.insert(notion_id.clone(), markdown);
            }
            Err(e) => println!("  ✗ Fehler beim Export aus Notion ({}): {}", notion_id, e),
        }
    }

    // 3. WICHTIG: Aktionen mit den frischen Daten neu berechnen!
    let sync_actions = helpers::compare_memories(&paperless_map, &notion_full);

    for (notion_id, action) in &sync_actions {
        match action {
            model::SyncAction::CreateInPaperless => {
                println!("➔ [NOTION-ID: {}]: Muss in Paperless erstellt werden.", notion_id);
                if let (Some((last_edited_time, title, hash)), Some(markdown)) =
                    (notion_full.get(notion_id), notion_content.get(notion_id))
                {
                    let _ = upload_to_paperless(
                        client,
                        paperless_url,
                        paperless_token,
                        notion_id,
                        last_edited_time,
                        title,
                        hash,
                        markdown,
                        fields
                    ).await;
                }
            }
            model::SyncAction::UpdateNotion => {
                println!("➔ [NOTION-ID: {}]: Notion-Eintrag veraltet. Update Notion!", notion_id);
            }
            model::SyncAction::UpdatePaperless => {
                println!("➔ [NOTION-ID: {}]: Paperless-Eintrag veraltet. Update Paperless!", notion_id);
                if let Some((paperless_id, _, _)) = paperless_map.get(notion_id) {
                    if let (Some((last_edited_time, title, hash)), Some(markdown)) =
                        (notion_full.get(notion_id), notion_content.get(notion_id))
                    {
                        println!("  - Lösche alte Version (Paperless ID: {})...", paperless_id);
                        let _ = delete_from_paperless(
                            client,
                            paperless_url,
                            paperless_token,
                            *paperless_id
                        ).await;
                        let unique_markdown = format!("{}\n\n---\n*Letztes Update in Notion: {}*", markdown, last_edited_time);
                        let _ = upload_to_paperless(
                            client,
                            paperless_url,
                            paperless_token,
                            notion_id,
                            last_edited_time,
                            title,
                            hash,
                            &unique_markdown,
                            fields
                        ).await;
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

#[cfg(test)]
mod tests {
    use super::{ base_domain, normalize_next_url, parse_duplicate_document_id };

    #[test]
    fn duplicate_id_wird_aus_paperless_fehlermeldung_geparst() {
        assert_eq!(
            parse_duplicate_document_id(
                "moin moin.md: Not consuming moin moin.md: It is a duplicate of moin moin (#327)."
            ),
            Some(327)
        );
    }

    #[test]
    fn andere_fehlermeldungen_liefern_keine_id() {
        assert_eq!(parse_duplicate_document_id("Timeout beim Verbindungsaufbau"), None);
    }

    #[test]
    fn next_url_uebernimmt_schema_der_konfiguration() {
        // Der Fall aus der Praxis: Paperless hinter einem Proxy ohne X-Forwarded-Proto
        // gibt http:// zurück, obwohl die Instanz unter https:// erreichbar ist.
        assert_eq!(
            normalize_next_url(
                "http://paperless.example.dev/api/documents/?page=2&page_size=100",
                "https://paperless.example.dev"
            ),
            "https://paperless.example.dev/api/documents/?page=2&page_size=100"
        );
    }

    #[test]
    fn lan_instanz_bleibt_http_mit_port() {
        assert_eq!(
            normalize_next_url(
                "http://paperless.local:8000/api/documents/?page=2",
                "http://paperless.local:8000"
            ),
            "http://paperless.local:8000/api/documents/?page=2"
        );
    }

    #[test]
    fn fehlender_slash_vor_query_wird_ergaenzt() {
        assert_eq!(
            normalize_next_url("https://p.example.dev/api/documents?page=2", "https://p.example.dev"),
            "https://p.example.dev/api/documents/?page=2"
        );
    }

    #[test]
    fn base_domain_schneidet_api_pfad_ab_und_behaelt_port() {
        assert_eq!(base_domain("http://paperless.local:8000/api/documents/"), "http://paperless.local:8000");
        assert_eq!(base_domain("https://paperless.example.dev/"), "https://paperless.example.dev");
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    println!("Sync Engine is running!");
    
    // Paperless & Notion Setup (Einmalig beim Start)
    // Timeout, damit ein hängender Request nicht den ganzen Dienst blockiert.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let paperless_url = std::env::var("PAPERLESS_URL").expect("PAPERLESS_URL must be set");
    let paperless_token = std::env::var("PAPERLESS_TOKEN").expect("PAPERLESS_TOKEN must be set");
    let notion_url = std::env::var("NOTION_URL").expect("NOTION_URL must be set");
    let notion_token = std::env::var("NOTION_TOKEN").expect("NOTION_TOKEN must be set");

    let sync_interval = Duration::from_secs(
        std::env::var("SYNC_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(300)
    );

    // Feld-IDs einmalig auflösen: sie sind pro Paperless-Instanz verschieden.
    // Schlägt das fehl, ist die Konfiguration kaputt -> lieber sofort abbrechen,
    // als in jedem Durchlauf ins Leere zu synchronisieren.
    println!("Prüfe Paperless Custom-Fields…");
    let fields = ensure_custom_fields(
        &client,
        &base_domain(&paperless_url),
        paperless_token.trim()
    ).await?;

    // ==================================================
    // DER REAKTIVE ZYKLUS (Hier beginnt die Schleife)
    // ==================================================
    loop {
        println!("\n--- Starte Synchronisation ---");

        // Ein Fehler (z. B. Netzwerk-Timeout, API 5xx) beendet NICHT mehr das Programm,
        // sondern nur diesen Durchlauf. Beim nächsten Intervall wird erneut versucht.
        if let Err(e) = run_sync_cycle(
            &client,
            &paperless_url,
            &paperless_token,
            &notion_url,
            &notion_token,
            &fields
        ).await {
            println!("  ✗ Synchronisation fehlgeschlagen: {}", e);
        }

        println!(
            "Abgeschlossene Synchronisation. Warte {} Sekunden bis zum nächsten Durchlauf...",
            sync_interval.as_secs()
        );
        sleep(sync_interval).await;
    }
}
