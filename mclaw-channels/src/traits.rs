//! Channel adapter trait.

use async_trait::async_trait;
use mclaw_gateway::protocol::ChannelKind;
use tokio_util::sync::CancellationToken;

/// Trait implemented by each chat platform adapter.
#[async_trait]
pub trait ChannelAdapter: Send + Sync + 'static {
    /// The channel kind this adapter handles.
    fn kind(&self) -> ChannelKind;

    /// Start the adapter — connect to gateway WS and begin listening.
    /// Runs until the cancellation token is triggered.
    async fn start(&self, gateway_url: String, shutdown: CancellationToken) -> anyhow::Result<()>;
}
