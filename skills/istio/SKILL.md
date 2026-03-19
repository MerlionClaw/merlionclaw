---
name: istio
description: Manage Istio service mesh - VirtualServices, DestinationRules, Gateways, traffic shifting
version: 0.1.0
permissions:
  - k8s:read
  - k8s:write
tools:
  - name: istio_list_virtualservices
    description: List VirtualServices in a namespace
    parameters:
      namespace:
        type: string
        default: default
  - name: istio_get_virtualservice
    description: Get VirtualService details with routing rules
    parameters:
      name:
        type: string
        required: true
      namespace:
        type: string
        default: default
  - name: istio_list_destinationrules
    description: List DestinationRules in a namespace
    parameters:
      namespace:
        type: string
        default: default
  - name: istio_list_gateways
    description: List Istio Gateways in a namespace
    parameters:
      namespace:
        type: string
        default: default
---

# Istio Skill

You help manage Istio service mesh resources. Guidelines:
- When listing resources, show hosts, gateways, and route weights
- For traffic shifting, always confirm current weights before modifying
- Warn about potential downtime when modifying VirtualServices
- Show mTLS status when relevant to security discussions
