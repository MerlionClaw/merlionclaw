//! Markdown file-based memory storage.

use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use tracing::{debug, info};

use crate::search::{MemoryHit, SearchIndex};

/// A diary entry for a specific date.
#[derive(Debug, Clone)]
pub struct DiaryEntry {
    /// The date of this entry.
    pub date: NaiveDate,
    /// The entry content.
    pub content: String,
}

/// Persistent memory store backed by markdown files and a tantivy search index.
pub struct MemoryStore {
    base_dir: PathBuf,
    index: SearchIndex,
}

impl MemoryStore {
    /// Create or open a memory store at the given directory.
    pub async fn new(base_dir: PathBuf) -> anyhow::Result<Self> {
        // Create directory structure
        tokio::fs::create_dir_all(base_dir.join("diary")).await?;
        tokio::fs::create_dir_all(base_dir.join("context")).await?;

        let index_dir = base_dir.join(".index");
        let index = SearchIndex::open(&index_dir)?;

        let store = Self { base_dir, index };

        // Rebuild index from existing files
        store.rebuild_index().await?;

        info!(dir = %store.base_dir.display(), "memory store initialized");
        Ok(store)
    }

    /// Add a long-term fact to MEMORY.md.
    pub async fn add_fact(&self, fact: &str) -> anyhow::Result<()> {
        let memory_file = self.base_dir.join("MEMORY.md");
        let mut content = self.read_file(&memory_file).await;

        // Add as a bullet point
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!("- {fact}\n"));

        tokio::fs::write(&memory_file, &content).await?;

        // Index the fact
        let facts = self.parse_facts(&content);
        let fact_idx = facts.len().saturating_sub(1);
        self.index
            .index_document("fact", &format!("fact_{fact_idx}"), fact)?;

        debug!(fact, "added fact");
        Ok(())
    }

    /// Remove a fact from MEMORY.md (by partial match).
    pub async fn remove_fact(&self, fact: &str) -> anyhow::Result<bool> {
        let memory_file = self.base_dir.join("MEMORY.md");
        let content = self.read_file(&memory_file).await;

        let fact_lower = fact.to_lowercase();
        let lines: Vec<&str> = content.lines().collect();
        let new_lines: Vec<&str> = lines
            .iter()
            .filter(|line| {
                let stripped = line.trim_start_matches("- ").trim();
                !stripped.to_lowercase().contains(&fact_lower)
            })
            .copied()
            .collect();

        let removed = new_lines.len() < lines.len();

        if removed {
            let new_content = new_lines.join("\n") + "\n";
            tokio::fs::write(&memory_file, &new_content).await?;
            self.rebuild_index().await?;
            debug!(fact, "removed fact");
        }

        Ok(removed)
    }

    /// Get all long-term facts.
    pub async fn get_facts(&self) -> anyhow::Result<Vec<String>> {
        let memory_file = self.base_dir.join("MEMORY.md");
        let content = self.read_file(&memory_file).await;
        Ok(self.parse_facts(&content))
    }

    /// Append an entry to today's diary.
    pub async fn append_diary(&self, entry: &str) -> anyhow::Result<()> {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let diary_file = self.base_dir.join("diary").join(format!("{today}.md"));

        let mut content = self.read_file(&diary_file).await;

        if content.is_empty() {
            content = format!("# Diary: {today}\n\n");
        }

        let timestamp = chrono::Utc::now().format("%H:%M:%S");
        content.push_str(&format!("- [{timestamp}] {entry}\n"));

        tokio::fs::write(&diary_file, &content).await?;

        self.index
            .index_document("diary", &today, entry)?;

        debug!(date = %today, "appended diary entry");
        Ok(())
    }

    /// Get diary entries for a date range.
    pub async fn get_diary(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> anyhow::Result<Vec<DiaryEntry>> {
        let mut entries = Vec::new();
        let mut date = from;

        while date <= to {
            let date_str = date.format("%Y-%m-%d").to_string();
            let diary_file = self.base_dir.join("diary").join(format!("{date_str}.md"));

            if diary_file.exists() {
                let content = tokio::fs::read_to_string(&diary_file).await?;
                entries.push(DiaryEntry {
                    date,
                    content,
                });
            }

            date += chrono::Duration::days(1);
        }

        Ok(entries)
    }

    /// Search all memory by query.
    pub async fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<MemoryHit>> {
        self.index.search(query, limit)
    }

    /// Save a session context snapshot.
    pub async fn save_context(&self, session_id: &str, summary: &str) -> anyhow::Result<()> {
        let safe_id = session_id.replace([':', '/', '\\'], "_");
        let context_file = self.base_dir.join("context").join(format!("{safe_id}.md"));

        let timestamp = chrono::Utc::now().to_rfc3339();
        let content = format!("# Session: {session_id}\nUpdated: {timestamp}\n\n{summary}\n");

        tokio::fs::write(&context_file, &content).await?;
        self.index
            .index_document("context", session_id, summary)?;

        debug!(session_id, "saved context");
        Ok(())
    }

    /// Rebuild the search index from all files.
    async fn rebuild_index(&self) -> anyhow::Result<()> {
        let memory_file = self.base_dir.join("MEMORY.md");
        let facts = if memory_file.exists() {
            let content = tokio::fs::read_to_string(&memory_file).await?;
            self.parse_facts(&content)
        } else {
            Vec::new()
        };

        let mut diary_entries = Vec::new();
        let diary_dir = self.base_dir.join("diary");
        if diary_dir.exists() {
            let mut dir = tokio::fs::read_dir(&diary_dir).await?;
            while let Some(entry) = dir.next_entry().await? {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "md") {
                    let date = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    let content = tokio::fs::read_to_string(&path).await?;
                    diary_entries.push((date, content));
                }
            }
        }

        self.index.rebuild(&facts, &diary_entries)?;
        Ok(())
    }

    fn parse_facts(&self, content: &str) -> Vec<String> {
        content
            .lines()
            .filter(|line| line.starts_with("- "))
            .map(|line| line.trim_start_matches("- ").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    async fn read_file(&self, path: &Path) -> String {
        tokio::fs::read_to_string(path).await.unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn temp_store() -> (MemoryStore, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let store = MemoryStore::new(dir.path().to_path_buf()).await.unwrap();
        (store, dir)
    }

    #[tokio::test]
    async fn test_add_and_get_facts() {
        let (store, _dir) = temp_store().await;

        store.add_fact("cluster is on EKS").await.unwrap();
        store.add_fact("uses Helm v3").await.unwrap();

        let facts = store.get_facts().await.unwrap();
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0], "cluster is on EKS");
        assert_eq!(facts[1], "uses Helm v3");
    }

    #[tokio::test]
    async fn test_remove_fact() {
        let (store, _dir) = temp_store().await;

        store.add_fact("keep this").await.unwrap();
        store.add_fact("remove this").await.unwrap();

        let removed = store.remove_fact("remove").await.unwrap();
        assert!(removed);

        let facts = store.get_facts().await.unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0], "keep this");
    }

    #[tokio::test]
    async fn test_append_diary() {
        let (store, _dir) = temp_store().await;
        store.append_diary("deployed nginx v1.25").await.unwrap();

        let today = chrono::Utc::now().date_naive();
        let entries = store.get_diary(today, today).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].content.contains("nginx"));
    }

    #[tokio::test]
    async fn test_search() {
        let (store, _dir) = temp_store().await;

        store.add_fact("production runs on EKS us-west-2").await.unwrap();
        store.add_fact("staging uses minikube").await.unwrap();

        let hits = store.search("EKS production", 5).await.unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].content.contains("EKS"));
    }

    #[tokio::test]
    async fn test_save_context() {
        let (store, _dir) = temp_store().await;

        store
            .save_context("telegram:123", "discussed pod restarts")
            .await
            .unwrap();

        let hits = store.search("pod restarts", 5).await.unwrap();
        assert!(!hits.is_empty());
    }
}
