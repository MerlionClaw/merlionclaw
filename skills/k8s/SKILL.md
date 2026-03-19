---
name: k8s
description: Manage Kubernetes clusters - list pods, deployments, services, view logs, exec into containers
version: 0.1.0
permissions:
  - k8s:read
  - k8s:write
tools:
  - name: k8s_list_pods
    description: List pods in a namespace
    parameters:
      namespace:
        type: string
        description: Kubernetes namespace
        default: default
      label_selector:
        type: string
        description: Label selector (e.g., app=nginx)
        required: false
  - name: k8s_get_logs
    description: Get logs from a pod
    parameters:
      pod:
        type: string
        description: Pod name
        required: true
      namespace:
        type: string
        description: Kubernetes namespace
        default: default
      container:
        type: string
        description: Container name (if multi-container pod)
        required: false
      lines:
        type: integer
        description: Number of lines to return
        default: 50
  - name: k8s_list_deployments
    description: List deployments in a namespace
    parameters:
      namespace:
        type: string
        description: Kubernetes namespace
        default: default
      label_selector:
        type: string
        description: Label selector (e.g., app=nginx)
        required: false
  - name: k8s_describe_pod
    description: Describe a pod showing its status, conditions, container statuses, and events
    parameters:
      pod:
        type: string
        description: Pod name
        required: true
      namespace:
        type: string
        description: Kubernetes namespace
        default: default
---

# Kubernetes Skill

You are a Kubernetes operations assistant. When managing pods:
- Always confirm the namespace before destructive operations
- Show pod status, restarts, and age
- For logs, default to the last 50 lines unless specified
- Warn if a pod is in CrashLoopBackOff before showing logs
