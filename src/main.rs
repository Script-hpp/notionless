mod notion;
mod paperless;
mod sync;

use std::time::Duration;
use tokio::time::sleep;

/// Resolves `$XDG_CONFIG_HOME`, falling back to `~/.config` per XDG convention.
fn xdg_config_dir() -> Option<std::path::PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(std::path::PathBuf::from(xdg));
    }
    std::env::var("HOME").ok().map(|home| std::path::PathBuf::from(home).join(".config"))
}

/// Loads config from `~/.config/notionless/.env` if present, otherwise from `.env` in
/// the current directory.
///
/// Deliberately not using `dotenvy::dotenv()` here: it walks up parent directories
/// looking for a `.env`, which is convenient inside this repo but a footgun for an
/// installed binary (a systemd service or a copy in `/usr/local/bin`) that could
/// silently pick up an unrelated `.env` from some ancestor directory instead of
/// failing cleanly.
fn load_config() {
    if let Some(path) = xdg_config_dir().map(|dir| dir.join("notionless").join(".env"))
        && dotenvy::from_path(&path).is_ok()
    {
        return;
    }
    let _ = dotenvy::from_filename(".env");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_config();
    println!("Sync engine is running!");

    // Timeout so a hanging request doesn't block the whole service.
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

    // Resolved once at startup: the field IDs differ per Paperless instance. If this
    // fails, the configuration is broken - better to abort now than sync into the void
    // on every cycle.
    println!("Checking Paperless custom fields…");
    let fields = paperless::ensure_custom_fields(
        &client,
        &paperless::base_domain(&paperless_url),
        paperless_token.trim()
    ).await?;

    loop {
        println!("\n--- Starting synchronization ---");

        // An error (e.g. network timeout, API 5xx) no longer terminates the process,
        // only this cycle. The next interval will retry.
        if let Err(e) = sync::run_sync_cycle(
            &client,
            &paperless_url,
            &paperless_token,
            &notion_url,
            &notion_token,
            &fields
        ).await {
            println!("  ✗ Synchronization failed: {}", e);
        }

        println!(
            "Synchronization complete. Waiting {} seconds until next run...",
            sync_interval.as_secs()
        );
        sleep(sync_interval).await;
    }
}
