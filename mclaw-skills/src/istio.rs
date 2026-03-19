//! Istio skill handler — manages Istio resources via kube-rs with dynamic API.

#[cfg(feature = "k8s")]
mod inner {
    use async_trait::async_trait;
    use kube::api::{Api, DynamicObject, ListParams};
    use kube::discovery::ApiResource;
    use kube::Client;
    use tracing::debug;

    use crate::registry::SkillHandler;

    /// Istio skill — manages Istio CRDs via the Kubernetes API.
    pub struct IstioSkill {
        client: Client,
    }

    impl IstioSkill {
        /// Create a new Istio skill handler.
        pub async fn new() -> anyhow::Result<Self> {
            let client = Client::try_default().await?;
            Ok(Self { client })
        }

        fn vs_api_resource() -> ApiResource {
            ApiResource {
                group: "networking.istio.io".to_string(),
                version: "v1".to_string(),
                api_version: "networking.istio.io/v1".to_string(),
                kind: "VirtualService".to_string(),
                plural: "virtualservices".to_string(),
            }
        }

        fn dr_api_resource() -> ApiResource {
            ApiResource {
                group: "networking.istio.io".to_string(),
                version: "v1".to_string(),
                api_version: "networking.istio.io/v1".to_string(),
                kind: "DestinationRule".to_string(),
                plural: "destinationrules".to_string(),
            }
        }

        fn gw_api_resource() -> ApiResource {
            ApiResource {
                group: "networking.istio.io".to_string(),
                version: "v1".to_string(),
                api_version: "networking.istio.io/v1".to_string(),
                kind: "Gateway".to_string(),
                plural: "gateways".to_string(),
            }
        }

        async fn list_dynamic(
            &self,
            namespace: &str,
            ar: ApiResource,
            kind_label: &str,
        ) -> anyhow::Result<String> {
            let api: Api<DynamicObject> =
                Api::namespaced_with(self.client.clone(), namespace, &ar);

            let list = api.list(&ListParams::default()).await?;

            if list.items.is_empty() {
                return Ok(format!("No {kind_label} found in namespace '{namespace}'."));
            }

            let mut lines = vec![format!("{:<30} {:<30} {}", "NAME", "HOSTS", "NAMESPACE")];

            for item in &list.items {
                let name = item.metadata.name.as_deref().unwrap_or("<unknown>");
                let hosts = item
                    .data
                    .get("spec")
                    .and_then(|s| s.get("hosts"))
                    .and_then(|h| h.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| "-".to_string());

                lines.push(format!("{:<30} {:<30} {}", name, hosts, namespace));
            }

            Ok(lines.join("\n"))
        }

        async fn list_virtualservices(&self, input: serde_json::Value) -> anyhow::Result<String> {
            let namespace = input["namespace"].as_str().unwrap_or("default");
            debug!(namespace, "listing VirtualServices");
            self.list_dynamic(namespace, Self::vs_api_resource(), "VirtualServices")
                .await
        }

        async fn get_virtualservice(&self, input: serde_json::Value) -> anyhow::Result<String> {
            let name = input["name"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("'name' parameter is required"))?;
            let namespace = input["namespace"].as_str().unwrap_or("default");

            let api: Api<DynamicObject> =
                Api::namespaced_with(self.client.clone(), namespace, &Self::vs_api_resource());

            let vs = api.get(name).await?;

            // Format the spec as readable YAML-like output
            let spec = vs.data.get("spec").cloned().unwrap_or_default();
            let formatted = serde_json::to_string_pretty(&spec)?;

            Ok(format!(
                "VirtualService: {name}\nNamespace: {namespace}\n\nSpec:\n{formatted}"
            ))
        }

        async fn list_destinationrules(&self, input: serde_json::Value) -> anyhow::Result<String> {
            let namespace = input["namespace"].as_str().unwrap_or("default");
            debug!(namespace, "listing DestinationRules");
            self.list_dynamic(namespace, Self::dr_api_resource(), "DestinationRules")
                .await
        }

        async fn list_gateways(&self, input: serde_json::Value) -> anyhow::Result<String> {
            let namespace = input["namespace"].as_str().unwrap_or("default");
            debug!(namespace, "listing Gateways");
            self.list_dynamic(namespace, Self::gw_api_resource(), "Gateways")
                .await
        }
    }

    #[async_trait]
    impl SkillHandler for IstioSkill {
        async fn execute(
            &self,
            tool_name: &str,
            input: serde_json::Value,
        ) -> anyhow::Result<String> {
            match tool_name {
                "istio_list_virtualservices" => self.list_virtualservices(input).await,
                "istio_get_virtualservice" => self.get_virtualservice(input).await,
                "istio_list_destinationrules" => self.list_destinationrules(input).await,
                "istio_list_gateways" => self.list_gateways(input).await,
                _ => Err(anyhow::anyhow!("unknown istio tool: {tool_name}")),
            }
        }
    }
}

#[cfg(feature = "k8s")]
pub use inner::IstioSkill;
