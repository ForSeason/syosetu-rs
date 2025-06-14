use std::sync::Arc;

use anyhow::{anyhow, Result};
use clap::Parser;
use reqwest::Client;
use scraper::{Html, Selector};

const SYOSETU_API_BASE: &str = "https://ncode.syosetu.com/";
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
AppleWebKit/537.36 (KHTML, like Gecko) \
Chrome/136.0.0.0 Safari/537.36 Edg/136.0.0.0";

const TRANSLATE_PROMPT: &str = r##"
请将以下日文内容完整、准确地翻译成中文。  
要求：  
1. 保持原文段落结构；  
2. 不要添加任何解释、注释或额外信息；  
3. **仅输出译文，不要输出原文或其他解释；**
4. 注重文章原本的表达，特别是对话需要准确反映语气与人物特点。

{}
"##;

const KEYWORD_PROMPT: &str = r##"
请根据以下已提取的翻译列表、日文原文和中文译文，从中找出新的专有名词（日文原文中的人名、地名、招式名、非常见物品名等），以及它们在译文中的对应中文译名。  
要求：  
1. 仅输出新的翻译对照，不要重复已提取条目；  
2. 输出格式为 JSONL，每行一个，例如：{"japanese": "トウリ", "chinese": "托莉"}；  
3. **不要添加任何说明、注释或其他额外内容。不要使用markdown格式或使用三引号将json包裹**

已提取的翻译列表：  
{existing_pairs}  

日文原文：  
{japanese_text}  

中文译文：  
{chinese_text}
"##;

const DEEPSEEK_API_BASE: &str = "https://api.deepseek.com/chat/completions";

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

struct SyosetuClient {
    client: Arc<Client>,
    api_key: String,
    model: String,
}

impl SyosetuClient {
    fn new(api_key: String, model: String) -> Self {
        SyosetuClient {
            client: Arc::new(Client::new()),
            api_key,
            model,
        }
    }

    async fn process_novel(&self, url: &str) -> Result<()> {
        // 获取目录页面
        let directory_html = self
            .client
            .get(url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await?
            .text()
            .await?;

        let document = Html::parse_document(&directory_html);
        let link_selector = Selector::parse("a.p-eplist__subtitle")
            .map_err(|e| anyhow!("selector parse error: {e}"))?;
        // 提取所有章节链接和文本
        let links: Vec<(String, String)> = document
            .select(&link_selector)
            .filter_map(|el| {
                let href = el.value().attr("href")?;
                let text = el
                    .text()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join("");
                Some((href.to_string(), text))
            })
            .collect();

        if let Some((path, _title)) = links.get(30) {
            let full_url = format!("{SYOSETU_API_BASE}{}", path);
            let content_html = self
                .client
                .get(&full_url)
                .header("User-Agent", USER_AGENT)
                .send()
                .await?
                .text()
                .await?;

            let document = Html::parse_document(&content_html);
            let body_selector = Selector::parse("div.p-novel__body")
                .map_err(|e| anyhow!("selector parse error: {e}"))?;

            if let Some(element) = document.select(&body_selector).next() {
                let content = element
                    .text()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n");
                let trans = self.translate_text(&content).await?;
                let new_keywords = self.extract_keywords(&trans, &content, Vec::new()).await?;
                println!("{trans}");
                for keyword in new_keywords {
                    println!("{keyword}")
                }
            }
        }

        Ok(())
    }

    async fn translate_text(&self, input: &str) -> Result<String> {
        let req = serde_json::json!({
           "model": self.model,
           "messages": [
               {"role": "user", "content": TRANSLATE_PROMPT.replace("{}", input)}
           ],
           "max_tokens": 8192,
           "temperature": 1.3,
           "stream": false,
        });
        let resp = self
            .client
            .post(DEEPSEEK_API_BASE)
            .json(&req)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;
        // println!("{}", resp.text().await.unwrap());
        // unimplemented!();
        let output = resp
            .json::<serde_json::Value>()
            .await?
            .pointer("/choices/0/message/content")
            .ok_or(anyhow!("deepseek api response api error"))?
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(output)
    }

    async fn extract_keywords(
        &self,
        zh: &str,
        jp: &str,
        keywords: Vec<String>,
    ) -> Result<Vec<String>> {
        let req = serde_json::json!({
           "model": self.model,
           "messages": [
               {"role": "user", "content": KEYWORD_PROMPT.replace("{existing_pairs}", &format!("{keywords:?}")).replace("{japanese_text}", jp).replace("{chineses_text}", zh)}
           ],
           "max_tokens": 8192,
           "temperature": 1.3,
           "stream": false,
        });
        let resp = self
            .client
            .post(DEEPSEEK_API_BASE)
            .json(&req)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;
        // println!("{}", resp.text().await.unwrap());
        // unimplemented!();
        let output = resp
            .json::<serde_json::Value>()
            .await?
            .pointer("/choices/0/message/content")
            .ok_or(anyhow!("deepseek api response api error"))?
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(output.split('\n').map(|s| s.to_string()).collect())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let client = SyosetuClient::new(args.api_key, args.model);
    client.process_novel(&args.url).await
}
