---
name: memory
description: Store and recall long-term facts about the user and their infrastructure
version: 0.1.0
permissions: []
tools:
  - name: memory_add_fact
    description: Store a long-term fact about the user or their infrastructure
    parameters:
      fact:
        type: string
        description: The fact to remember
        required: true
  - name: memory_search
    description: Search memory for past conversations and stored facts
    parameters:
      query:
        type: string
        description: Search query
        required: true
      limit:
        type: integer
        description: Maximum results to return
        default: 5
  - name: memory_list_facts
    description: List all stored long-term facts
    parameters: {}
  - name: memory_remove_fact
    description: Remove a stored fact by partial match
    parameters:
      fact:
        type: string
        description: The fact (or part of it) to remove
        required: true
---

# Memory Skill

You can store and recall facts about the user and their infrastructure.
When the user tells you something important about their setup, use memory_add_fact to remember it.
When asked about past interactions or infrastructure details, use memory_search first.
