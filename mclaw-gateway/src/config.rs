//! Gateway configuration.

use serde::Deserialize;
use std::net::IpAddr;

/// Gateway server configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    /// Host IP to bind to.
    #[serde(default = "default_host")]
    pub host: IpAddr,
    /// Port to listen on.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Session timeout in seconds (default: 24 hours).
    #[serde(default = "default_session_timeout")]
    pub session_timeout_secs: u64,
}

fn default_host() -> IpAddr {
    IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
}

fn default_port() -> u16 {
    18789
}

fn default_session_timeout() -> u64 {
    86400 // 24 hours
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            session_timeout_secs: default_session_timeout(),
        }
    }
}
