use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;

/// 用于持久化保存专有名词翻译表的抽象接口
pub trait KeywordStore: Send + Sync {
    /// 读取指定小说的翻译表
    fn load(&self, novel_id: &str) -> Result<HashMap<String, String>>;
    /// 保存翻译表
    fn save(&self, novel_id: &str, keywords: &HashMap<String, String>) -> Result<()>;
}

/// 将翻译表存储为 JSON 文件
pub struct JsonStore {
    path: PathBuf,
}

/// 缓存章节翻译内容的接口
pub trait TranslationStore: Send + Sync {
    /// 读取指定章节的翻译内容
    fn load(&self, novel_id: &str, chapter: &str) -> Result<Option<String>>;
    /// 保存章节翻译
    fn save(&self, novel_id: &str, chapter: &str, text: &str) -> Result<()>;
    /// 列出所有已缓存章节路径
    fn list(&self, novel_id: &str) -> Result<Vec<String>>;
}

/// 简单的 JSON 文件实现，用于保存章节翻译
pub struct JsonTranslationStore {
    path: PathBuf,
}

impl JsonTranslationStore {
    /// 创建一个新的 JSON 翻译存储
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        JsonTranslationStore { path: path.into() }
    }

    /// 读取整个文件并解析为嵌套的 HashMap
    fn read_all(&self) -> HashMap<String, HashMap<String, String>> {
        if let Ok(content) = fs::read_to_string(&self.path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        }
    }

    /// 将内存中的数据写回文件，先写入临时文件再原子覆盖，避免意外写入
    fn write_all(&self, data: &HashMap<String, HashMap<String, String>>) -> Result<()> {
        let s = serde_json::to_string_pretty(data)?;
        let tmp_path = self.path.with_extension("tmp");
        fs::write(&tmp_path, s)?;
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

impl JsonStore {
    /// 创建一个新的 JSON 存储
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        JsonStore { path: path.into() }
    }

    /// 读取文件中的全部内容
    fn read_all(&self) -> HashMap<String, HashMap<String, String>> {
        if let Ok(content) = fs::read_to_string(&self.path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        }
    }

    /// 写回全部数据
    fn write_all(&self, data: &HashMap<String, HashMap<String, String>>) -> Result<()> {
        let s = serde_json::to_string_pretty(data)?;
        let tmp_path = self.path.with_extension("tmp");
        fs::write(&tmp_path, s)?;
        fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }
}

impl KeywordStore for JsonStore {
    fn load(&self, novel_id: &str) -> Result<HashMap<String, String>> {
        let all = self.read_all();
        Ok(all.get(novel_id).cloned().unwrap_or_default())
    }

    fn save(&self, novel_id: &str, keywords: &HashMap<String, String>) -> Result<()> {
        let mut all = self.read_all();
        let entry = all.entry(novel_id.to_string()).or_default();
        for (jp, zh) in keywords {
            entry.entry(jp.clone()).or_insert(zh.clone());
        }
        self.write_all(&all)
    }
}

impl TranslationStore for JsonTranslationStore {
    fn load(&self, novel_id: &str, chapter: &str) -> Result<Option<String>> {
        let all = self.read_all();
        Ok(all
            .get(novel_id)
            .and_then(|m| m.get(chapter).cloned()))
    }

    fn save(&self, novel_id: &str, chapter: &str, text: &str) -> Result<()> {
        let mut all = self.read_all();
        let entry = all.entry(novel_id.to_string()).or_default();
        entry.insert(chapter.to_string(), text.to_string());
        self.write_all(&all)
    }

    fn list(&self, novel_id: &str) -> Result<Vec<String>> {
        let all = self.read_all();
        Ok(all
            .get(novel_id)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default())
    }
}
