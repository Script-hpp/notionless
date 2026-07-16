pub mod model;

pub use model::CustomFieldIds;
use model::{ CustomFieldDefinition, CustomFieldListResponse, PaperlessDocument, PaperlessResponse };

use reqwest::multipart;
use std::collections::HashMap;

// Custom-field names in Paperless. The numeric IDs differ per instance and are
// resolved at startup via these names (see `ensure_custom_fields`).
const FIELD_NAME_NOTION_ID: &str = "notion_id";
const FIELD_NAME_LAST_EDITED: &str = "notion_last_edited";
const FIELD_NAME_CONTENT_HASH: &str = "notion_content_hash";

/// A Notion page's known state in Paperless: which document holds it, and what content
/// hash was uploaded last.
pub struct DocumentRecord {
    pub document_id: i64,
    pub last_edited_time: String,
    pub content_hash: String,
}

/// Strips a path suffix (e.g. `/api/documents/`) from the configured URL, leaving the
/// Paperless instance's base. Scheme and port are left untouched.
pub fn base_domain(paperless_url: &str) -> String {
    let clean = paperless_url.trim().trim_end_matches('/');
    clean.split("/api").next().unwrap_or(clean).trim_end_matches('/').to_string()
}

/// Rewrites a Paperless pagination `next` URL onto the configured base.
///
/// Paperless builds `next` from what it believes its own host to be. Behind a reverse
/// proxy without `X-Forwarded-Proto`, that's `http://` even though the instance is
/// actually served over `https://`. Following it as-is makes reqwest drop the
/// Authorization header on the http->https redirect (a scheme change counts as a
/// different origin), and Paperless responds 401.
///
/// So we only take the path + query from `next` and append them to the configured
/// base: a LAN instance stays http, a proxied instance stays https.
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

    // Some setups omit the trailing slash before the query string.
    if let Some(query_idx) = url.find('?') {
        if !url[..query_idx].ends_with('/') {
            url.insert(query_idx, '/');
        }
    } else if !url.ends_with('/') {
        url.push('/');
    }

    url
}

/// Resolves this Paperless instance's custom-field IDs by name, creating any that are
/// missing. This lets the sync run against any instance without manual configuration.
pub async fn ensure_custom_fields(
    client: &reqwest::Client,
    base: &str,
    token: &str
) -> Result<CustomFieldIds, Box<dyn std::error::Error>> {
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
            return Err(format!("Failed to read custom fields (status {}): {}", status, text).into());
        }

        let page = response.json::<CustomFieldListResponse>().await?;
        for field in page.results {
            existing.insert(field.name, field.id);
        }
        current_url = page.next.map(|next| normalize_next_url(&next, base));
    }

    Ok(CustomFieldIds {
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
        println!("  ✓ Found custom field '{}' (ID {}).", name, id);
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
        return Err(format!("Failed to create custom field '{}' (status {}): {}", name, status, text).into());
    }

    let field = response.json::<CustomFieldDefinition>().await?;
    println!("  + Created custom field '{}' in Paperless (ID {}).", name, field.id);
    Ok(field.id)
}

/// Fetches the current Notion-page -> Paperless-document mapping, read off every
/// document that carries our custom fields.
pub async fn fetch_memory(
    client: &reqwest::Client,
    start_url: &str,
    token: &str,
    fields: &CustomFieldIds
) -> Result<HashMap<String, DocumentRecord>, Box<dyn std::error::Error>> {
    let mut records: HashMap<String, DocumentRecord> = HashMap::new();
    let base = base_domain(start_url);
    let mut current_url: Option<String> = Some(start_url.to_string());

    while let Some(url) = current_url {
        let response = client
            .get(&url)
            .header("Authorization", format!("Token {}", token))
            .send().await?
            .json::<PaperlessResponse>().await?;

        for doc in response.results {
            let mut notion_id: Option<String> = None;
            let mut last_edited_time: Option<String> = None;
            let mut content_hash: Option<String> = None;
            for custom_field in doc.custom_fields {
                if custom_field.field == fields.notion_id {
                    notion_id = custom_field.value;
                } else if custom_field.field == fields.last_edited {
                    last_edited_time = custom_field.value;
                } else if custom_field.field == fields.content_hash {
                    content_hash = custom_field.value;
                }
            }

            if let (Some(id), Some(last_edited_time)) = (notion_id, last_edited_time) {
                records.insert(id, DocumentRecord {
                    document_id: doc.id,
                    last_edited_time,
                    content_hash: content_hash.unwrap_or_default(),
                });
            }
        }

        current_url = response.next.map(|next| normalize_next_url(&next, &base));
    }

    Ok(records)
}

/// Moves a Paperless document to trash and immediately empties it.
///
/// A plain DELETE only moves the document to trash; Paperless' duplicate check still
/// searches the trash, so re-uploading the same content would otherwise be rejected
/// with "existing document is in the trash". Emptying the trash avoids that.
pub async fn delete_document(
    client: &reqwest::Client,
    paperless_url: &str,
    token: &str,
    document_id: i64
) -> Result<(), Box<dyn std::error::Error>> {
    let clean_token = token.trim();
    let base = base_domain(paperless_url);
    let delete_url = format!("{}/api/documents/{}/", base, document_id);

    let response = client
        .delete(&delete_url)
        .header("Authorization", format!("Token {}", clean_token))
        .header("Accept", "application/json")
        .header("Referer", &delete_url)
        .header("Origin", &base)
        .send().await?;

    if response.status().is_success() || response.status() == 204 {
        println!("  ✓ Moved old document (ID: {}) to trash.", document_id);

        let trash_url = format!("{}/api/trash/", base);
        let trash_body = serde_json::json!({ "action": "empty", "documents": [document_id] });
        let trash_response = client
            .post(&trash_url)
            .header("Authorization", format!("Token {}", clean_token))
            .header("Accept", "application/json")
            .json(&trash_body)
            .send().await?;

        if trash_response.status().is_success() {
            println!("  ✓ Permanently removed document (ID: {}) from trash.", document_id);
        } else {
            let status = trash_response.status();
            println!(
                "  ⚠ Failed to empty trash for ID {} (status: {}). Re-upload may be rejected as a duplicate.",
                document_id, status
            );
            if let Ok(text) = trash_response.text().await {
                println!("    Reason: {}", text);
            }
        }
    } else {
        println!("  ✗ Failed to delete document {} (status: {}):", document_id, response.status());
        if let Ok(text) = response.text().await {
            println!("    Reason: {}", text);
        }
    }

    Ok(())
}

/// Result of waiting for a Paperless task to finish.
enum TaskResult {
    Success,
    /// The task failed; carries Paperless' plain-text reason (e.g. a duplicate message).
    Failure(String),
    Timeout,
}

/// Paperless processes uploads asynchronously. `post_document` only returns a task ID;
/// whether the document was actually ingested (or e.g. rejected as a duplicate) only
/// shows up afterwards in the task status. We poll for it and always log the real
/// outcome.
async fn wait_for_task(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    task_id: &str,
    title: &str
) -> Result<TaskResult, Box<dyn std::error::Error>> {
    let tasks_url = format!("{}/api/tasks/?task_id={}", base, task_id);

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
                    println!("  ✓ Paperless successfully ingested '{}'.", title);
                    return Ok(TaskResult::Success);
                }
                "FAILURE" => {
                    let result = task
                        .get("result")
                        .and_then(|r| r.as_str())
                        .unwrap_or("(no reason given)")
                        .to_string();
                    println!("  ✗ Paperless REJECTED '{}' (task {}): {}", title, task_id, result);
                    return Ok(TaskResult::Failure(result));
                }
                _ => {} // PENDING / STARTED / RECEIVED -> keep waiting
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    println!("  ⚠ Task {} for '{}' did not complete in time (timeout while waiting).", task_id, title);
    Ok(TaskResult::Timeout)
}

/// Extracts the Paperless document ID from a duplicate-rejection message, e.g.
/// "It is a duplicate of moin moin (#327)." -> Some(327).
fn parse_duplicate_document_id(message: &str) -> Option<i64> {
    let start = message.rfind("(#")? + 2;
    let end = start + message[start..].find(')')?;
    message[start..end].trim().parse().ok()
}

/// The Notion-side identity of a page being synced to Paperless: enough to tag or
/// re-tag a document without needing its full content.
pub struct NotionPageRef<'a> {
    pub notion_id: &'a str,
    pub last_edited_time: &'a str,
    pub content_hash: &'a str,
}

/// Links an already-existing Paperless document to a Notion page after the fact.
///
/// Kicks in when `post_document` rejects a document as a duplicate because it already
/// exists without a Notion link (e.g. from before notionless was used, or after a
/// custom-field migration). Without this, the sync would try to re-upload it - and get
/// rejected as a duplicate - on every cycle, forever.
///
/// Existing custom fields are read and merged in rather than overwritten: Paperless'
/// PATCH replaces the entire custom_fields list, so a bare `{"custom_fields": [...]}`
/// with only our three fields would wipe out any other, manually maintained fields.
async fn adopt_existing(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    document_id: i64,
    fields: &CustomFieldIds,
    page: &NotionPageRef<'_>
) -> Result<(), Box<dyn std::error::Error>> {
    let document_url = format!("{}/api/documents/{}/", base, document_id);

    let existing = client
        .get(&document_url)
        .header("Authorization", format!("Token {}", token))
        .header("Accept", "application/json")
        .send().await?
        .json::<PaperlessDocument>().await?;

    let our_field_ids = [fields.notion_id, fields.last_edited, fields.content_hash];
    let mut merged: Vec<serde_json::Value> = existing.custom_fields
        .into_iter()
        .filter(|cf| !our_field_ids.contains(&cf.field))
        .map(|cf| serde_json::json!({ "field": cf.field, "value": cf.value }))
        .collect();
    merged.push(serde_json::json!({ "field": fields.notion_id, "value": page.notion_id }));
    merged.push(serde_json::json!({ "field": fields.last_edited, "value": page.last_edited_time }));
    merged.push(serde_json::json!({ "field": fields.content_hash, "value": page.content_hash }));

    let response = client
        .patch(&document_url)
        .header("Authorization", format!("Token {}", token))
        .header("Accept", "application/json")
        .header("Referer", &document_url)
        .header("Origin", base)
        .json(&serde_json::json!({ "custom_fields": merged }))
        .send().await?;

    if response.status().is_success() {
        println!("  ✓ Linked existing document (ID: {}) to the Notion page.", document_id);
    } else {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        println!("  ✗ Failed to link document {} (status {}): {}", document_id, status, text);
    }

    Ok(())
}

/// Uploads a Notion page's exported Markdown to Paperless, tagged with the custom
/// fields that link it back to the Notion page.
pub async fn upload(
    client: &reqwest::Client,
    paperless_url: &str,
    token: &str,
    page: &NotionPageRef<'_>,
    title: &str,
    markdown_content: &str,
    fields: &CustomFieldIds
) -> Result<(), Box<dyn std::error::Error>> {
    let clean_token = token.trim();
    let base = base_domain(paperless_url);
    let upload_url = format!("{}/api/documents/post_document/", base);

    let file_part = multipart::Part::bytes(markdown_content.to_string().into_bytes())
        .file_name(format!("{}.md", title))
        .mime_str("text/markdown")?;

    let custom_fields_json = serde_json::json!({
        fields.notion_id.to_string(): page.notion_id,
        fields.last_edited.to_string(): page.last_edited_time,
        fields.content_hash.to_string(): page.content_hash,
    }).to_string();
    let form = multipart::Form::new()
        .part("document", file_part)
        .text("title", title.to_string())
        .text("custom_fields", custom_fields_json);

    let response = client
        .post(&upload_url)
        .header("Authorization", format!("Token {}", clean_token))
        .header("Accept", "application/json")
        .header("Referer", &upload_url)
        .header("Origin", &base)
        .multipart(form)
        .send().await?;

    if response.status().is_success() {
        // The body is the task ID (a JSON string). "Accepted" != "ingested": the real
        // duplicate/error info only shows up in the task status afterwards.
        let task_id: String = response.json().await.unwrap_or_default();
        let task_id = task_id.trim().to_string();
        if task_id.is_empty() {
            println!("  ⚠ Upload of '{}' accepted, but no task ID received.", title);
        } else {
            println!("  … Upload of '{}' accepted (task {}). Waiting for processing…", title, task_id);
            let task_result = wait_for_task(client, &base, clean_token, &task_id, title).await;
            if let Ok(TaskResult::Failure(message)) = task_result
                && let Some(document_id) = parse_duplicate_document_id(&message)
            {
                println!(
                    "  ↻ '{}' already exists in Paperless with identical content (ID: {}). Linking...",
                    title, document_id
                );
                let _ = adopt_existing(client, &base, clean_token, document_id, fields, page).await;
            }
        }
    } else {
        let status = response.status();
        let error_text = response.text().await?;
        println!("  ✗ Failed to upload '{}' (status: {}):", title, status);
        println!("    Reason: {}", error_text);
        println!("    URL was: {}", upload_url);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ base_domain, normalize_next_url, parse_duplicate_document_id };

    #[test]
    fn duplicate_id_is_parsed_from_paperless_error_message() {
        assert_eq!(
            parse_duplicate_document_id(
                "moin moin.md: Not consuming moin moin.md: It is a duplicate of moin moin (#327)."
            ),
            Some(327)
        );
    }

    #[test]
    fn other_error_messages_yield_no_id() {
        assert_eq!(parse_duplicate_document_id("Connection timed out"), None);
    }

    #[test]
    fn next_url_takes_scheme_from_configured_base() {
        // The real-world case: Paperless behind a proxy without X-Forwarded-Proto
        // returns http:// even though the instance is reachable over https://.
        assert_eq!(
            normalize_next_url(
                "http://paperless.example.dev/api/documents/?page=2&page_size=100",
                "https://paperless.example.dev"
            ),
            "https://paperless.example.dev/api/documents/?page=2&page_size=100"
        );
    }

    #[test]
    fn lan_instance_stays_http_with_port() {
        assert_eq!(
            normalize_next_url(
                "http://paperless.local:8000/api/documents/?page=2",
                "http://paperless.local:8000"
            ),
            "http://paperless.local:8000/api/documents/?page=2"
        );
    }

    #[test]
    fn missing_slash_before_query_is_added() {
        assert_eq!(
            normalize_next_url("https://p.example.dev/api/documents?page=2", "https://p.example.dev"),
            "https://p.example.dev/api/documents/?page=2"
        );
    }

    #[test]
    fn base_domain_strips_api_path_and_keeps_port() {
        assert_eq!(base_domain("http://paperless.local:8000/api/documents/"), "http://paperless.local:8000");
        assert_eq!(base_domain("https://paperless.example.dev/"), "https://paperless.example.dev");
    }
}
