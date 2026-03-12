use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const MAX_ENTRIES: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub timestamp: DateTime<Utc>,
    pub role: String,
    pub content: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Memory {
    pub entries: Vec<Entry>,
}

impl Memory {
    fn path() -> PathBuf {
        let dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("virus");
        fs::create_dir_all(&dir).ok();
        dir.join("memory.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        if path.exists() {
            let data = fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Ok(data) = serde_json::to_string_pretty(self) {
            fs::write(&path, data).ok();
        }
    }

    pub fn append(&mut self, role: &str, content: &str) {
        self.entries.push(Entry {
            timestamp: Utc::now(),
            role: role.to_string(),
            content: content.to_string(),
        });
        // keep only recent entries
        if self.entries.len() > MAX_ENTRIES {
            let drain = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..drain);
        }
        self.save();
    }

    /// Format recent memory for the LLM context window
    pub fn recent_context(&self, n: usize) -> Vec<Entry> {
        let start = self.entries.len().saturating_sub(n);
        self.entries[start..].to_vec()
    }
}
