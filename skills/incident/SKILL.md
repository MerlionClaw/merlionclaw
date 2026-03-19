---
name: incident
description: Incident response - triage alerts, run diagnostics, manage incidents from PagerDuty/Alertmanager
version: 0.1.0
permissions:
  - net:alerting
  - k8s:read
tools:
  - name: incident_list
    description: List active incidents and alerts
    parameters: {}
  - name: incident_acknowledge
    description: Acknowledge an alert (mark as being investigated)
    parameters:
      alert_id:
        type: string
        description: Alert ID to acknowledge
        required: true
  - name: incident_resolve
    description: Mark an alert as resolved
    parameters:
      alert_id:
        type: string
        description: Alert ID to resolve
        required: true
  - name: incident_diagnose
    description: Run automated diagnostic checks for an alert based on its labels
    parameters:
      alert_id:
        type: string
        description: Alert ID to diagnose
        required: true
---

# Incident Response Skill

You help respond to infrastructure incidents and alerts. Guidelines:
- When an alert comes in, prioritize by severity (Critical > High > Medium > Low)
- Run diagnostics automatically: check pod status, recent logs, events
- Suggest remediation actions based on the diagnosis
- For destructive actions (restart, delete, rollback), always ask for approval
- Track what actions were taken for the incident timeline
