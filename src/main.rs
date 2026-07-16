mod notion;
mod paperless;
mod sync;

use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
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
