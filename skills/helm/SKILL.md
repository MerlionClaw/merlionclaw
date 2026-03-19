---
name: helm
description: Manage Helm releases - list, install, upgrade, rollback, check status and history
version: 0.1.0
permissions:
  - k8s:read
  - exec:helm
tools:
  - name: helm_list
    description: List Helm releases
    parameters:
      namespace:
        type: string
        description: Namespace (omit for all namespaces)
        required: false
      filter:
        type: string
        description: Filter by release name substring
        required: false
  - name: helm_status
    description: Get detailed status of a Helm release
    parameters:
      release:
        type: string
        description: "Name of the Helm release to check status for"
        required: true
      namespace:
        type: string
        description: Kubernetes namespace of the release
        default: default
  - name: helm_history
    description: Show revision history of a Helm release
    parameters:
      release:
        type: string
        description: Name of the Helm release
        required: true
      namespace:
        type: string
        description: Kubernetes namespace of the release
        default: default
  - name: helm_values
    description: Get current values for a Helm release
    parameters:
      release:
        type: string
        description: Name of the Helm release
        required: true
      namespace:
        type: string
        description: Kubernetes namespace of the release
        default: default
  - name: helm_upgrade
    description: Upgrade a Helm release with new values
    parameters:
      release:
        type: string
        description: Name of the Helm release to upgrade
        required: true
      chart:
        type: string
        description: Chart reference (e.g., bitnami/nginx, ./my-chart)
        required: true
      namespace:
        type: string
        description: Kubernetes namespace of the release
        default: default
      set:
        type: array
        description: Individual key=value overrides
        required: false
  - name: helm_rollback
    description: Rollback a Helm release to a previous revision
    parameters:
      release:
        type: string
        description: Name of the Helm release to rollback
        required: true
      revision:
        type: integer
        description: Target revision number (omit for previous)
        required: false
      namespace:
        type: string
        description: Kubernetes namespace of the release
        default: default
  - name: helm_uninstall
    description: Uninstall a Helm release
    parameters:
      release:
        type: string
        description: Name of the Helm release to uninstall
        required: true
      namespace:
        type: string
        description: Kubernetes namespace of the release
        default: default
---

# Helm Skill

You help manage Helm releases on Kubernetes clusters. Guidelines:
- Always show the current revision and app version when reporting status
- Before upgrade/install, confirm the chart version and key value changes
- For rollbacks, show the history first so the user can pick the right revision
- Warn if a release is in a failed state before attempting operations
