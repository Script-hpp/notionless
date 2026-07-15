use std::collections::HashMap;
use chrono::{DateTime, NaiveDate, Utc};
use crate::model::SyncAction;

pub fn parse_paperless_date(date_str: &str) -> Option<DateTime<Utc>> {
   NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .ok()
        .map(|date| date.and_hms_opt(0, 0, 0).unwrap().and_utc())    
}

pub fn parse_notion_date(date_str: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(date_str)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

pub fn compare_memories(
    paperless: &HashMap<String, String>,
    notion: &HashMap<String, String>,
) -> HashMap<String, SyncAction> {
    let mut actions = HashMap::new();

    // Wir gehen durch alle Einträge, die wir in Notion gefunden haben
    for (notion_id, notion_time_str) in notion {
        let notion_time = match parse_notion_date(notion_time_str) {
            Some(dt) => dt,
            None => continue,
        };

        if let Some(paperless_time_str) = paperless.get(notion_id) {
            // ID existiert in beiden Systemen -> Zeiten vergleichen!
            if let Some(paperless_time) = parse_paperless_date(paperless_time_str) {
                if paperless_time > notion_time {
                    actions.insert(notion_id.clone(), SyncAction::UpdateNotion);
                } else if notion_time > paperless_time {
                    actions.insert(notion_id.clone(), SyncAction::UpdatePaperless);
                } else {
                    actions.insert(notion_id.clone(), SyncAction::UpToDate);
                }
            }
        } else {
            // ID existiert in Notion, aber noch nicht in Paperless
            actions.insert(notion_id.clone(), SyncAction::CreateInPaperless);
        }
    }

    actions
}