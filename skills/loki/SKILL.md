---
name: loki
description: Query logs from Loki and interact with Grafana dashboards and alerts
version: 0.1.0
permissions:
  - net:grafana
tools:
  - name: loki_query
    description: Execute a LogQL query against Loki
    parameters:
      query:
        type: string
        description: "LogQL query, e.g., {namespace=\"production\",app=\"nginx\"} |= \"error\""
        required: true
      limit:
        type: integer
        description: Max number of log lines to return
        default: 50
  - name: loki_query_range
    description: Query logs within a specific time range
    parameters:
      query:
        type: string
        description: "LogQL query to execute over the time range"
        required: true
      start:
        type: string
        description: "Start time (e.g., '1h ago', '2026-03-19T10:00:00Z')"
        required: true
      end:
        type: string
        description: "End time (default: now)"
        required: false
      limit:
        type: integer
        description: Max number of log lines to return
        default: 100
  - name: loki_labels
    description: List available log stream label names
    parameters: {}
  - name: loki_label_values
    description: List values for a specific label
    parameters:
      label:
        type: string
        description: Label name to list values for
        required: true
  - name: loki_tail
    description: Get the most recent log lines for a stream selector
    parameters:
      query:
        type: string
        description: "Stream selector, e.g., {app=\"nginx\"}"
        required: true
      lines:
        type: integer
        description: Number of recent log lines to return
        default: 20
---

# Loki / Grafana Skill

You help query logs and monitor alerts. Guidelines:
- When showing logs, format them with timestamps aligned
- Highlight error-level lines or patterns like "panic", "fatal", "OOMKilled"
- For broad queries, suggest narrowing with label selectors or pattern filters
- When errors are found, proactively suggest related K8s commands to investigate
- Always mention the time range of returned logs
