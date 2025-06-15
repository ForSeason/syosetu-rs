use anyhow::Result;
use clap::Parser;
use log::{error, LevelFilter};
use std::fs::OpenOptions;
use env_logger::{Builder, Target};

use crate::app::App;
use crate::memory::{JsonStore, JsonTranslationStore};
use crate::syosetu::{NcodeSite, OrgSite, NovelSite, Translator};

mod app;
mod memory;
mod syosetu;
mod ui;

/// 命令行参数定义
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

/// 解析参数并启动应用
#[tokio::main]
async fn main() -> Result<()> {
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("app.log")?;
    Builder::from_default_env()
        .filter_level(LevelFilter::Info)
        .target(Target::Pipe(Box::new(log_file)))
        .init();
    let args = Args::parse();
    let novel_id = args
        .url
        .trim_end_matches('/')
        .split('/')
        .last()
        .unwrap_or("novel")
        .to_string();

    let translator = Translator::new(args.api_key, args.model);
    let site: Box<dyn NovelSite> = if args.url.contains("syosetu.org") {
        Box::new(OrgSite::new())
    } else {
        Box::new(NcodeSite::new())
    };
    let store = JsonStore::new("keywords.json");
    let trans_store = JsonTranslationStore::new("translations.json");
    let app = App::new(novel_id);
    let result = app
        .run(&args.url, site.as_ref(), &translator, &store, &trans_store)
        .await;
    if let Err(ref e) = result {
        error!("Application error: {:?}", e);
    }
    result
}
