use anyhow::Result;
use clap::Parser;

use crate::app::App;
use crate::memory::JsonStore;
use crate::syosetu::SyosetuClient;

mod app;
mod memory;
mod syosetu;
mod ui;

#[derive(Parser, Debug)]
#[command(author, version, about = "syosetu scraper")]
struct Args {
    /// Novel index page url
    #[arg(long)]
    url: String,

    /// DeepSeek API key
    #[arg(long)]
    api_key: String,

    /// Model name used when calling DeepSeek API
    #[arg(long, default_value = "deepseek-chat")]
    model: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let novel_id = args
        .url
        .trim_end_matches('/')
        .split('/')
        .last()
        .unwrap_or("novel")
        .to_string();

    let client = SyosetuClient::new(args.api_key, args.model);
    let store = JsonStore::new("keywords.json");
    let app = App::new(novel_id);
    app.run(&args.url, &client, &store).await
}
