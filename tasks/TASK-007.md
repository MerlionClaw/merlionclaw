# TASK-007: Memory System

## Objective
Implement persistent memory with markdown file storage, full-text search via tantivy, and daily diary generation. Memory allows the agent to recall past conversations, user preferences, and infrastructure context across sessions.

## Dependencies
- TASK-006 must be complete (working MVP)

## Steps

### 1. Memory store (mclaw-memory/src/store.rs)

Markdown-based storage compatible with OpenClaw's memory format:

```
~/.merlionclaw/memory/
├── MEMORY.md           # long-term facts (curated)
├── diary/
│   ├── 2026-03-19.md   # daily log
│   ├── 2026-03-20.md
│   └── ...
└── context/
    └── {session_id}.md  # per-session context snapshots
```

```rust
pub struct MemoryStore {
    base_dir: PathBuf,
    index: SearchIndex,
}

impl MemoryStore {
    pub async fn new(base_dir: PathBuf) -> Result<Self>;

    /// Add a long-term fact to MEMORY.md
    pub async fn add_fact(&self, fact: &str) -> Result<()>;

    /// Remove a fact from MEMORY.md
    pub async fn remove_fact(&self, fact: &str) -> Result<()>;

    /// Get all long-term facts
    pub async fn get_facts(&self) -> Result<Vec<String>>;

    /// Append to today's diary
    pub async fn append_diary(&self, entry: &str) -> Result<()>;

    /// Get diary entries for a date range
    pub async fn get_diary(&self, from: NaiveDate, to: NaiveDate) -> Result<Vec<DiaryEntry>>;

    /// Search all memory (facts + diary) by query
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryHit>>;

    /// Save a session context snapshot
    pub async fn save_context(&self, session_id: &str, summary: &str) -> Result<()>;
}

pub struct DiaryEntry {
    pub date: NaiveDate,
    pub content: String,
}

pub struct MemoryHit {
    pub source: MemorySource,  // Fact, Diary(date), Context(session_id)
    pub content: String,
    pub score: f32,
}
```

### 2. Search index (mclaw-memory/src/search.rs)

Use tantivy for full-text search:

```rust
pub struct SearchIndex {
    index: tantivy::Index,
    reader: IndexReader,
}

impl SearchIndex {
    /// Create or open index at the given path
    pub fn open(index_dir: &Path) -> Result<Self>;

    /// Index a document (fact or diary entry)
    pub fn index_document(&self, doc: MemoryDocument) -> Result<()>;

    /// Search with BM25 scoring
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryHit>>;

    /// Rebuild index from all markdown files
    pub fn rebuild(&self, memory_dir: &Path) -> Result<usize>;
}
```

Schema fields:
- `source_type`: STRING (fact | diary | context)
- `source_id`: STRING (date for diary, "memory" for facts, session_id for context)
- `content`: TEXT (full-text indexed, stored)
- `timestamp`: DATE

### 3. Memory-aware agent loop

Update `mclaw-agent` to use memory:

1. Before calling LLM, search memory for relevant context:
   ```rust
   let hits = memory.search(&user_message, 5).await?;
   ```
2. Inject memory into the system prompt:
   ```
   ## Relevant Memory
   - [fact] User prefers Helm v3 over v2
   - [2026-03-18] Upgraded nginx deployment to v1.25 in production namespace
   - [fact] Production cluster is on EKS us-west-2
   ```
3. After conversation turn, extract and store key facts via LLM:
   - Add a tool `memory_store` that the LLM can call to save important info
   - Or do it passively: after each conversation, ask LLM to extract facts

### 4. Memory management tools

Expose these as LLM tools so the user can manage memory through chat:

```yaml
tools:
  - name: memory_add_fact
    description: Store a long-term fact about the user or their infrastructure
    parameters:
      fact:
        type: string
        required: true
  - name: memory_search
    description: Search memory for past conversations and stored facts
    parameters:
      query:
        type: string
        required: true
      limit:
        type: integer
        default: 5
  - name: memory_list_facts
    description: List all stored long-term facts
  - name: memory_remove_fact
    description: Remove a stored fact
    parameters:
      fact:
        type: string
        required: true
```

### 5. Daily diary generation

At the end of each day (or when the agent shuts down), summarize the day's interactions:

```rust
pub async fn generate_daily_diary(
    store: &MemoryStore,
    conversations: &[ConversationSummary],
) -> Result<()> {
    // Use LLM to summarize: "Summarize today's interactions in 3-5 bullet points"
    // Append to diary/{date}.md
}
```

Can also be triggered manually via `/diary` command.

### 6. Special commands

- `/memory` → show recent facts and today's diary
- `/memory search <query>` → search memory
- `/forget <fact>` → remove a fact
- `/diary` → show today's diary

## Validation

```bash
cargo test -p mclaw-memory

# Integration test:
cargo run -- run
# Telegram:
You: "remember that our prod cluster is on EKS us-west-2"
Bot: "Got it. I'll remember that your production cluster is on EKS us-west-2."

You: "what do you know about our infrastructure?"
Bot: "Based on what I remember: Your production cluster is on EKS us-west-2..."

# Verify file:
cat ~/.merlionclaw/memory/MEMORY.md
# Should contain: "Production cluster is on EKS us-west-2"

You: /memory
Bot: "Facts: 1. Production cluster is on EKS us-west-2 | Diary: (today's entries)"
```

## Output

A persistent memory system that survives restarts, supports full-text search, and allows the agent to build up context about the user's infrastructure over time.
