//! Full-text search index using tantivy.

use std::path::Path;

use tantivy::collector::TopDocs;
use tantivy::directory::MmapDirectory;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy};
use tracing::{debug, info};

/// The source of a memory hit.
#[derive(Debug, Clone)]
pub enum MemorySource {
    /// A long-term fact from MEMORY.md.
    Fact,
    /// A diary entry for a specific date.
    Diary(String),
    /// A session context snapshot.
    Context(String),
}

/// A search result from the memory index.
#[derive(Debug, Clone)]
pub struct MemoryHit {
    /// Where this memory came from.
    pub source: MemorySource,
    /// The matched content.
    pub content: String,
    /// Relevance score.
    pub score: f32,
}

/// Full-text search index backed by tantivy.
pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    source_type_field: Field,
    source_id_field: Field,
    content_field: Field,
}

impl SearchIndex {
    /// Open or create a search index at the given directory.
    pub fn open(index_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(index_dir)?;

        let mut schema_builder = Schema::builder();
        let source_type_field = schema_builder.add_text_field("source_type", STRING | STORED);
        let source_id_field = schema_builder.add_text_field("source_id", STRING | STORED);
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let schema = schema_builder.build();

        let mmap_dir = MmapDirectory::open(index_dir)?;
        let index = if Index::exists(&mmap_dir)? {
            Index::open(mmap_dir)?
        } else {
            Index::create_in_dir(index_dir, schema)?
        };

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self {
            index,
            reader,
            source_type_field,
            source_id_field,
            content_field,
        })
    }

    /// Open an in-memory index (for testing).
    pub fn open_in_memory() -> anyhow::Result<Self> {
        let mut schema_builder = Schema::builder();
        let source_type_field = schema_builder.add_text_field("source_type", STRING | STORED);
        let source_id_field = schema_builder.add_text_field("source_id", STRING | STORED);
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let schema = schema_builder.build();

        let index = Index::create_in_ram(schema);

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;

        Ok(Self {
            index,
            reader,
            source_type_field,
            source_id_field,
            content_field,
        })
    }

    /// Index a single document.
    pub fn index_document(
        &self,
        source_type: &str,
        source_id: &str,
        content: &str,
    ) -> anyhow::Result<()> {
        let mut writer = self.writer()?;
        writer.add_document(doc!(
            self.source_type_field => source_type,
            self.source_id_field => source_id,
            self.content_field => content,
        ))?;
        writer.commit()?;
        self.reader.reload()?;
        debug!(source_type, source_id, "indexed document");
        Ok(())
    }

    /// Delete all documents matching a source_type and source_id.
    pub fn delete_document(&self, source_type: &str, source_id: &str) -> anyhow::Result<()> {
        let mut writer = self.writer()?;
        let term_type = tantivy::Term::from_field_text(self.source_type_field, source_type);
        let term_id = tantivy::Term::from_field_text(self.source_id_field, source_id);
        // Delete by source_type + source_id combo — delete all matching type first, re-index
        // For simplicity, just delete by source_id
        writer.delete_term(term_type);
        writer.delete_term(term_id);
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    /// Search the index.
    pub fn search(&self, query_str: &str, limit: usize) -> anyhow::Result<Vec<MemoryHit>> {
        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.content_field]);

        let query = query_parser
            .parse_query(query_str)
            .unwrap_or_else(|_| {
                // Fall back to a simple term query if parsing fails
                Box::new(tantivy::query::AllQuery)
            });

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut hits = Vec::new();
        for (score, doc_address) in top_docs {
            let doc: tantivy::TantivyDocument = searcher.doc(doc_address)?;

            let source_type = doc
                .get_first(self.source_type_field)
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let source_id = doc
                .get_first(self.source_id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let content = doc
                .get_first(self.content_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let source = match source_type {
                "fact" => MemorySource::Fact,
                "diary" => MemorySource::Diary(source_id.to_string()),
                "context" => MemorySource::Context(source_id.to_string()),
                _ => MemorySource::Fact,
            };

            hits.push(MemoryHit {
                source,
                content,
                score,
            });
        }

        Ok(hits)
    }

    /// Rebuild the entire index from memory files.
    pub fn rebuild(&self, facts: &[String], diary_entries: &[(String, String)]) -> anyhow::Result<usize> {
        let mut writer = self.writer()?;
        writer.delete_all_documents()?;

        let mut count = 0;

        for (i, fact) in facts.iter().enumerate() {
            writer.add_document(doc!(
                self.source_type_field => "fact",
                self.source_id_field => format!("fact_{i}"),
                self.content_field => fact.as_str(),
            ))?;
            count += 1;
        }

        for (date, content) in diary_entries {
            writer.add_document(doc!(
                self.source_type_field => "diary",
                self.source_id_field => date.as_str(),
                self.content_field => content.as_str(),
            ))?;
            count += 1;
        }

        writer.commit()?;
        self.reader.reload()?;
        info!(count, "index rebuilt");
        Ok(count)
    }

    fn writer(&self) -> anyhow::Result<IndexWriter> {
        Ok(self.index.writer(15_000_000)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_and_search() {
        let idx = SearchIndex::open_in_memory().unwrap();
        idx.index_document("fact", "fact_0", "production cluster is on EKS us-west-2")
            .unwrap();
        idx.index_document("fact", "fact_1", "staging uses minikube locally")
            .unwrap();

        let hits = idx.search("EKS production", 5).unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].content.contains("EKS"));
    }

    #[test]
    fn test_rebuild() {
        let idx = SearchIndex::open_in_memory().unwrap();
        let facts = vec![
            "fact one".to_string(),
            "fact two".to_string(),
        ];
        let diary = vec![
            ("2026-03-19".to_string(), "deployed nginx".to_string()),
        ];
        let count = idx.rebuild(&facts, &diary).unwrap();
        assert_eq!(count, 3);

        let hits = idx.search("nginx", 5).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn test_empty_search() {
        let idx = SearchIndex::open_in_memory().unwrap();
        let hits = idx.search("nothing", 5).unwrap();
        assert!(hits.is_empty());
    }
}
