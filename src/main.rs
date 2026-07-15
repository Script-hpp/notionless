mod model;

use model::{PaperlessDocument, PaperlessResponse};




#[tokio::main]
async fn main()-> Result<(), Box<dyn std::error::Error>> {
    println!("Sync Engine is running!");
    let client = reqwest::Client::new();

    Ok(())
}
