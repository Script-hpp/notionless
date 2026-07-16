use crate::notion;
use crate::paperless::{ self, CustomFieldIds, NotionPageRef };
use chrono::{ DateTime, Utc };
use sha2::{ Digest, Sha256 };
use std::collections::HashMap;

/// What to do with a given Notion page during this sync cycle.
#[derive(Debug)]
enum SyncAction {
    UpdateNotion, // Paperless is newer
    UpdatePaperless, // Notion is newer
    CreateInPaperless, // Only exists in Notion
    UpToDate,
}

/// A fully exported Notion page: metadata plus rendered Markdown and its content hash.
struct ExportedPage {
    last_edited_time: String,
    title: String,
    content_hash: String,
    markdown: String,
}

fn parse_notion_date(date_str: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(date_str).ok().map(|dt| dt.with_timezone(&Utc))
}

/// SHA-256 of the exported Markdown content, as a hex string.
/// The authoritative change signal, independent of Notion's minute-level timestamp
/// rounding.
fn compute_content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn compare_memories(
    paperless: &HashMap<String, paperless::DocumentRecord>,
    notion: &HashMap<String, ExportedPage>
) -> HashMap<String, SyncAction> {
    let mut actions = HashMap::new();

    for (notion_id, page) in notion {
        if let Some(record) = paperless.get(notion_id) {
            // The content hash is the authoritative change signal: matching hashes mean
            // identical content, regardless of what the timestamps say.
            if !record.content_hash.is_empty() && record.content_hash == page.content_hash {
                actions.insert(notion_id.clone(), SyncAction::UpToDate);
                continue;
            }

            // Hashes differ (or Paperless has no hash yet) - the timestamp only
            // decides the direction of the sync.
            let notion_time = parse_notion_date(&page.last_edited_time);
            let paperless_time = parse_notion_date(&record.last_edited_time);

            match (notion_time, paperless_time) {
                (Some(nt), Some(pt)) if pt > nt => {
                    println!("  [DEBUG] Paperless is newer than Notion (hash differs).");
                    actions.insert(notion_id.clone(), SyncAction::UpdateNotion);
                }
                _ => {
                    // Notion is newer, or same-minute timestamps with differing content.
                    println!("  [DEBUG] Notion content differs! Triggering UpdatePaperless.");
                    actions.insert(notion_id.clone(), SyncAction::UpdatePaperless);
                }
            }
        } else {
            actions.insert(notion_id.clone(), SyncAction::CreateInPaperless);
        }
    }

    actions
}

/// Runs exactly one synchronization cycle.
/// Returns an error instead of terminating the process - the main loop catches it and
/// continues at the next interval.
pub async fn run_sync_cycle(
    client: &reqwest::Client,
    paperless_url: &str,
    paperless_token: &str,
    notion_url: &str,
    notion_token: &str,
    fields: &CustomFieldIds
) -> Result<(), Box<dyn std::error::Error>> {
    // Always re-fetch both sides on every cycle; nothing here is cached across runs.
    let notion_pages = notion::fetch_memory(client, notion_url, notion_token).await?;
    let paperless_map = paperless::fetch_memory(client, paperless_url, paperless_token, fields).await?;

    let mut exported: HashMap<String, ExportedPage> = HashMap::new();
    for (notion_id, page) in &notion_pages {
        match notion::export_page_content(client, notion_id, notion_token).await {
            Ok(markdown) => {
                let content_hash = compute_content_hash(&markdown);
                exported.insert(notion_id.clone(), ExportedPage {
                    last_edited_time: page.last_edited_time.clone(),
                    title: page.title.clone(),
                    content_hash,
                    markdown,
                });
            }
            Err(e) => println!("  ✗ Failed to export from Notion ({}): {}", notion_id, e),
        }
    }

    let sync_actions = compare_memories(&paperless_map, &exported);

    for (notion_id, action) in &sync_actions {
        let Some(page) = exported.get(notion_id) else { continue };

        match action {
            SyncAction::CreateInPaperless => {
                println!("➔ [NOTION-ID: {}]: Needs to be created in Paperless.", notion_id);
                let page_ref = NotionPageRef {
                    notion_id,
                    last_edited_time: &page.last_edited_time,
                    content_hash: &page.content_hash,
                };
                let _ = paperless::upload(
                    client, paperless_url, paperless_token, &page_ref, &page.title, &page.markdown, fields
                ).await;
            }
            SyncAction::UpdateNotion => {
                println!("➔ [NOTION-ID: {}]: Notion entry is outdated. Update Notion!", notion_id);
            }
            SyncAction::UpdatePaperless => {
                println!("➔ [NOTION-ID: {}]: Paperless entry is outdated. Updating Paperless!", notion_id);
                if let Some(record) = paperless_map.get(notion_id) {
                    println!("  - Deleting old version (Paperless ID: {})...", record.document_id);
                    let _ = paperless::delete_document(client, paperless_url, paperless_token, record.document_id).await;

                    let unique_markdown = format!(
                        "{}\n\n---\n*Last updated in Notion: {}*",
                        page.markdown, page.last_edited_time
                    );
                    let page_ref = NotionPageRef {
                        notion_id,
                        last_edited_time: &page.last_edited_time,
                        content_hash: &page.content_hash,
                    };
                    let _ = paperless::upload(
                        client, paperless_url, paperless_token, &page_ref, &page.title, &unique_markdown, fields
                    ).await;
                }
            }
            SyncAction::UpToDate => {
                println!("➔ [NOTION-ID: {}]: Already up to date.", notion_id);
            }
        }
    }

    Ok(())
}
