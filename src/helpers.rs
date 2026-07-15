use std::collections::HashMap;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use crate::model::SyncAction;


pub fn parse_notion_date(date_str: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(date_str)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// SHA-256 des exportierten Markdown-Inhalts als Hex-String.
/// Maßgeblich für die Änderungserkennung, unabhängig von Notions Minuten-Rundung.
pub fn compute_content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn compare_memories(
    // notion_id -> (paperless_id, last_edited_time, content_hash)
    paperless: &HashMap<String, (i64, String, String)>,
    // notion_id -> (last_edited_time, title, content_hash)
    notion: &HashMap<String, (String, String, String)>,
) -> HashMap<String, SyncAction> {
    let mut actions = HashMap::new();

    // Wir gehen durch alle Einträge, die wir in Notion gefunden haben
    for (notion_id, (notion_time_str, _title, notion_hash)) in notion {
        if let Some((_paperless_id, paperless_time_str, paperless_hash)) = paperless.get(notion_id) {
            // 1. Der Inhalts-Hash ist die maßgebliche Änderungserkennung.
            //    Gleicher Hash => Inhalt identisch, egal was die Zeitstempel sagen.
            if !paperless_hash.is_empty() && paperless_hash == notion_hash {
                actions.insert(notion_id.clone(), SyncAction::UpToDate);
                continue;
            }

            // 2. Hashes weichen ab (oder Paperless hat noch keinen Hash gespeichert).
            //    Der Zeitstempel entscheidet nur noch über die Richtung.
            let notion_time = parse_notion_date(notion_time_str);
            let paperless_time = parse_notion_date(paperless_time_str);

            match (notion_time, paperless_time) {
                (Some(nt), Some(pt)) if pt > nt => {
                    println!("  [DEBUG] Paperless ist neuer als Notion (Hash weicht ab).");
                    actions.insert(notion_id.clone(), SyncAction::UpdateNotion);
                }
                _ => {
                    // Notion neuer ODER gleiche Minute mit abweichendem Inhalt
                    // (genau der Fall, der vorher als UpToDate durchrutschte).
                    println!("  [DEBUG] Notion-Inhalt weicht ab! Trigger UpdatePaperless.");
                    actions.insert(notion_id.clone(), SyncAction::UpdatePaperless);
                }
            }
        } else {
            actions.insert(notion_id.clone(), SyncAction::CreateInPaperless);
        }
    }

    actions
}
