use std::sync::Arc;

use anyhow::{anyhow, Result};
use reqwest::Client;
use scraper::{Html, Selector};
use async_trait::async_trait;

/// 发送请求时使用的 UA 字符串
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36 Edg/136.0.0.0";

const TRANSLATE_PROMPT: &str = r##"请将以下日文内容完整、准确地翻译成中文。
要求：
1. 保持原文段落结构；
2. 不要添加任何解释、注释或额外信息；
3. **仅输出译文，不要输出原文或其他解释；**
4. 注重文章原本的表达，特别是对话需要准确反映语气与人物特点。

{}"##;

const KEYWORD_PROMPT: &str = r##"请根据以下已提取的翻译列表、日文原文和中文译文，
从中找出新的专有名词（日文原文中的人名、地名、招式名、非常见物品名等），以及它们
在译文中的对应中文译名。
要求：
1. 仅输出新的翻译对照，不要重复已提取条目；
2. 输出格式为 JSONL，每行一个，例如:{\"japanese\":\"トウリ\",\"chinese\":\"托莉\"}；
3. **不要添加任何说明、注释或其他额外内容。不要使用markdown格式或使用三引号将json包裹**

已提取的翻译列表:
{existing_pairs}

日文原文:
{japanese_text}

中文译文:
{chinese_text}"##;

const DEEPSEEK_API_BASE: &str = "https://api.deepseek.com/chat/completions";

/// 目录中每个章节的基本信息
#[derive(Clone)]
pub struct Chapter {
    /// 章节的完整网址
    pub path: String,
    /// 章节标题
    pub title: String,
}

/// 提供翻译服务的客户端
pub struct Translator {
    client: Arc<Client>,
    api_key: String,
    model: String,
}

impl Translator {
    /// 创建新的翻译客户端
    pub fn new(api_key: String, model: String) -> Self {
        Translator {
            client: Arc::new(Client::new()),
            api_key,
            model,
        }
    }

    /// 调用 DeepSeek 接口翻译文本
    pub async fn translate_text(
        &self,
        input: &str,
        keywords: &[(String, String)],
    ) -> Result<String> {
        let known = if keywords.is_empty() {
            String::new()
        } else {
            let pairs = keywords
                .iter()
                .map(|(jp, zh)| format!("{jp}:{zh}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("已知翻译对照：{pairs}\n")
        };
        let content = format!("{known}{input}");
        let req = serde_json::json!({
           "model": self.model,
           "messages": [
               {"role": "user", "content": TRANSLATE_PROMPT.replace("{}", &content)}
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

    /// 从翻译结果中进一步提取新的专有名词对照
    pub async fn extract_keywords(
        &self,
        zh: &str,
        jp: &str,
        keywords: Vec<String>,
    ) -> Result<Vec<String>> {
        let req = serde_json::json!({
           "model": self.model,
           "messages": [
               {"role": "user", "content": KEYWORD_PROMPT.replace("{existing_pairs}", &format!("{keywords:?}")).replace("{japanese_text}", jp).replace("{chinese_text}", zh)}
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

/// 抽象小说站点需要实现的接口
#[async_trait::async_trait]
pub trait NovelSite: Send + Sync {
    /// 根据目录页地址抓取章节列表
    async fn fetch_directory(&self, url: &str) -> Result<Vec<Chapter>>;
    /// 下载并解析单章正文
    async fn fetch_chapter(&self, url: &str) -> Result<String>;
}

/// ncode.syosetu.com 的实现
pub struct NcodeSite {
    client: Arc<Client>,
}

impl NcodeSite {
    pub fn new() -> Self {
        NcodeSite {
            client: Arc::new(Client::new()),
        }
    }
}

#[async_trait]
impl NovelSite for NcodeSite {
    async fn fetch_directory(&self, url: &str) -> Result<Vec<Chapter>> {
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
        let links: Vec<Chapter> = document
            .select(&link_selector)
            .filter_map(|el| {
                let href = el.value().attr("href")?;
                let text = el
                    .text()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join("");
                let full = if href.starts_with("http") {
                    href.to_string()
                } else {
                    format!("https://ncode.syosetu.com{href}")
                };
                Some(Chapter { path: full, title: text })
            })
            .collect();
        Ok(links)
    }

    async fn fetch_chapter(&self, url: &str) -> Result<String> {
        let content_html = self
            .client
            .get(url)
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
            Ok(content)
        } else {
            Err(anyhow!("body not found"))
        }
    }
}

/// syosetu.org 的实现
pub struct OrgSite {
    client: Arc<Client>,
}

impl OrgSite {
    pub fn new() -> Self {
        OrgSite {
            client: Arc::new(Client::new()),
        }
    }
}

#[async_trait]
impl NovelSite for OrgSite {
    async fn fetch_directory(&self, url: &str) -> Result<Vec<Chapter>> {
        let directory_html = self
            .client
            .get(url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await?
            .text()
            .await?;
        let document = Html::parse_document(&directory_html);
        let selector = Selector::parse("div.ss table a[href$='.html']")
            .map_err(|e| anyhow!("selector parse error: {e}"))?;
        let base = url.trim_end_matches('/');
        let base = format!("{}/", base);
        let links: Vec<Chapter> = document
            .select(&selector)
            .filter_map(|el| {
                let href = el.value().attr("href")?;
                let title = el.text().collect::<Vec<_>>().join("");
                let full = if href.starts_with("http") {
                    href.to_string()
                } else {
                    format!("{}{}", base, href.trim_start_matches("./"))
                };
                Some(Chapter {
                    path: full,
                    title: title.trim().to_string(),
                })
            })
            .collect();
        Ok(links)
    }

    async fn fetch_chapter(&self, url: &str) -> Result<String> {
        let content_html = self
            .client
            .get(url)
            .header("User-Agent", USER_AGENT)
            .send()
            .await?
            .text()
            .await?;
        let document = Html::parse_document(&content_html);
        let body_selector = Selector::parse("div#honbun")
            .map_err(|e| anyhow!("selector parse error: {e}"))?;
        if let Some(element) = document.select(&body_selector).next() {
            let content = element
                .text()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(content)
        } else {
            Err(anyhow!("body not found"))
        }
    }
}
