//! Memory skill handler.

use std::sync::Arc;

use async_trait::async_trait;
use mclaw_memory::store::MemoryStore;
use tracing::debug;

use crate::registry::SkillHandler;

/// Memory skill — manages long-term facts and search.
pub struct MemorySkill {
    store: Arc<MemoryStore>,
}

impl MemorySkill {
    /// Create a new memory skill handler.
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl SkillHandler for MemorySkill {
    async fn execute(&self, tool_name: &str, input: serde_json::Value) -> anyhow::Result<String> {
        match tool_name {
            "memory_add_fact" => {
                let fact = input["fact"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("'fact' parameter is required"))?;
                self.store.add_fact(fact).await?;
                debug!(fact, "fact stored");
                Ok(format!("Stored: {fact}"))
            }
            "memory_search" => {
                let query = input["query"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("'query' parameter is required"))?;
                let limit = input["limit"].as_u64().unwrap_or(5) as usize;

                let hits = self.store.search(query, limit).await?;
                if hits.is_empty() {
                    Ok("No matching memories found.".to_string())
                } else {
                    let lines: Vec<String> = hits
                        .iter()
                        .enumerate()
                        .map(|(i, hit)| {
                            let source = match &hit.source {
                                mclaw_memory::search::MemorySource::Fact => "[fact]".to_string(),
                                mclaw_memory::search::MemorySource::Diary(d) => {
                                    format!("[diary:{d}]")
                                }
                                mclaw_memory::search::MemorySource::Context(s) => {
                                    format!("[context:{s}]")
                                }
                            };
                            format!("{}. {} {}", i + 1, source, hit.content)
                        })
                        .collect();
                    Ok(lines.join("\n"))
                }
            }
            "memory_list_facts" => {
                let facts = self.store.get_facts().await?;
                if facts.is_empty() {
                    Ok("No facts stored yet.".to_string())
                } else {
                    let lines: Vec<String> = facts
                        .iter()
                        .enumerate()
                        .map(|(i, f)| format!("{}. {f}", i + 1))
                        .collect();
                    Ok(lines.join("\n"))
                }
            }
            "memory_remove_fact" => {
                let fact = input["fact"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("'fact' parameter is required"))?;
                let removed = self.store.remove_fact(fact).await?;
                if removed {
                    Ok(format!("Removed fact matching: {fact}"))
                } else {
                    Ok(format!("No fact found matching: {fact}"))
                }
            }
            _ => Err(anyhow::anyhow!("unknown memory tool: {tool_name}")),
        }
    }
}
