use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;

pub trait KeywordStore: Send + Sync {
    fn load(&self, novel_id: &str) -> Result<HashMap<String, String>>;
    fn save(&self, novel_id: &str, keywords: &HashMap<String, String>) -> Result<()>;
}

pub struct JsonStore {
    path: PathBuf,
}

impl JsonStore {
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        JsonStore { path: path.into() }
    }

    fn read_all(&self) -> HashMap<String, HashMap<String, String>> {
        if let Ok(content) = fs::read_to_string(&self.path) {
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            HashMap::new()
        }
    }

    fn write_all(&self, data: &HashMap<String, HashMap<String, String>>) -> Result<()> {
        let s = serde_json::to_string_pretty(data)?;
        fs::write(&self.path, s)?;
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
