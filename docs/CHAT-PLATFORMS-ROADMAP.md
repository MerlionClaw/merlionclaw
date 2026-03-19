# MerlionClaw Chat Platforms Roadmap

## Current Status (5 Supported)

| Platform | Transport | Status | Config |
|----------|-----------|--------|--------|
| Telegram | Long polling (teloxide) | ✅ Shipped | `TELEGRAM_BOT_TOKEN` |
| Slack | Socket Mode WebSocket | ✅ Shipped | `SLACK_APP_TOKEN` + `SLACK_BOT_TOKEN` |
| Discord | Gateway WebSocket (serenity) | ✅ Shipped | `DISCORD_BOT_TOKEN` |
| WhatsApp | Webhook + Meta Cloud API | ✅ Shipped | `WHATSAPP_ACCESS_TOKEN` + phone_number_id |
| Microsoft Teams | Webhook + Bot Framework REST | ✅ Shipped | `TEAMS_APP_ID` + `TEAMS_APP_PASSWORD` |

---

## Phase 1: Enterprise & Workspace (High Priority)

These platforms are commonly used in enterprise DevOps/SRE teams.

### Google Chat
- **Transport:** Webhook (incoming) + REST API (outgoing)
- **Auth:** Google Service Account / OAuth2
- **Why:** Google Workspace teams need native integration
- **Effort:** Medium — similar to Teams webhook pattern
- **Config:** `GOOGLE_CHAT_SERVICE_ACCOUNT_KEY` + space ID

### Mattermost
- **Transport:** WebSocket (incoming) + REST API (outgoing)
- **Auth:** Bot token or personal access token
- **Why:** Self-hosted Slack alternative, popular in security-conscious orgs
- **Effort:** Medium — WebSocket event stream + REST replies
- **Config:** `MATTERMOST_URL` + `MATTERMOST_BOT_TOKEN`

---

## Phase 2: Open Source & Self-Hosted

Platforms favored by privacy-conscious and open-source communities.

### Matrix
- **Transport:** Client-Server API (long-poll sync or SSE)
- **Auth:** Access token or SSO
- **Why:** Federated, self-hosted, E2E encrypted — popular in open source
- **Effort:** Medium-High — matrix-sdk crate available, E2E crypto is complex
- **Config:** `MATRIX_HOMESERVER_URL` + `MATRIX_ACCESS_TOKEN`

### IRC
- **Transport:** Raw TCP with IRC protocol
- **Auth:** NickServ registration
- **Why:** Still used in many DevOps/SRE communities (Libera.Chat, OFTC)
- **Effort:** Low — simple text protocol, no special crate needed
- **Config:** IRC server + nick + channels

### Nextcloud Talk
- **Transport:** Polling + REST API
- **Auth:** App password
- **Why:** Self-hosted collaboration suite, common in EU/government
- **Effort:** Low — simple REST API
- **Config:** `NEXTCLOUD_URL` + `NEXTCLOUD_TOKEN`

---

## Phase 3: Asia-Pacific Markets

Platforms dominant in specific Asian markets.

### LINE
- **Transport:** Webhook (incoming) + Messaging API (outgoing)
- **Auth:** Channel access token
- **Why:** Dominant in Japan, Taiwan, Thailand (~200M users)
- **Effort:** Low — straightforward webhook + REST pattern
- **Config:** `LINE_CHANNEL_ACCESS_TOKEN` + `LINE_CHANNEL_SECRET`

### Feishu / Lark
- **Transport:** WebSocket or webhook + REST API
- **Auth:** App ID + App Secret
- **Why:** ByteDance's enterprise messenger, growing in China/SEA
- **Effort:** Medium — similar to Slack but different auth flow
- **Config:** `FEISHU_APP_ID` + `FEISHU_APP_SECRET`

### Zalo
- **Transport:** Webhook + REST API
- **Auth:** Bot API token
- **Why:** Dominant messenger in Vietnam (~75M users)
- **Effort:** Low — simple webhook pattern
- **Config:** `ZALO_BOT_TOKEN`

### DingTalk
- **Transport:** Webhook + REST API
- **Auth:** App key + App secret
- **Why:** Alibaba's enterprise messenger, widely used in China
- **Effort:** Low-Medium
- **Config:** `DINGTALK_APP_KEY` + `DINGTALK_APP_SECRET`

---

## Phase 4: Specialized & Niche

### Signal
- **Transport:** signal-cli (CLI wrapper over REST API)
- **Auth:** Linked device pairing
- **Why:** End-to-end encrypted, used by security-conscious SREs
- **Effort:** Medium — depends on external signal-cli binary
- **Config:** signal-cli server URL

### BlueBubbles (iMessage)
- **Transport:** REST API via BlueBubbles macOS proxy
- **Auth:** BlueBubbles server password
- **Why:** iMessage access for Apple ecosystem users
- **Effort:** Low — simple REST client
- **Config:** `BLUEBUBBLES_URL` + `BLUEBUBBLES_PASSWORD`
- **Limitation:** Requires macOS host running BlueBubbles

### Twitch
- **Transport:** IRC over WebSocket (TMI)
- **Auth:** OAuth token
- **Why:** DevOps streamers, live incident response demos
- **Effort:** Low — IRC-based protocol
- **Config:** `TWITCH_OAUTH_TOKEN` + channel name

---

## Phase 5: Decentralized & Experimental

### Nostr
- **Transport:** WebSocket to relays
- **Auth:** NIP-01 keypair
- **Why:** Censorship-resistant, decentralized — niche but growing
- **Effort:** Medium — custom relay protocol
- **Config:** `NOSTR_PRIVATE_KEY` + relay URLs

### Tlon (Urbit)
- **Transport:** Urbit API
- **Auth:** Ship address + access code
- **Why:** Decentralized computing community
- **Effort:** High — Urbit-specific API
- **Config:** Urbit ship address

### Synology Chat
- **Transport:** Webhook + REST API
- **Auth:** Bot token
- **Why:** NAS-based team chat for Synology users
- **Effort:** Low — simple REST pattern
- **Config:** `SYNOLOGY_CHAT_URL` + `SYNOLOGY_BOT_TOKEN`

---

## Phase 6: Web & Embeddable

### Webchat (Built-in)
- **Transport:** WebSocket (already exists as gateway /ws endpoint)
- **Auth:** Session-based
- **Why:** Embed MerlionClaw in any web dashboard
- **Effort:** Low — gateway already supports this, just needs a web UI
- **Status:** Gateway WS endpoint exists, web frontend needed

---

## Implementation Priority Summary

| Priority | Platform | Market | Effort |
|----------|----------|--------|--------|
| 🔴 P1 | Google Chat | Enterprise | Medium |
| 🔴 P1 | Mattermost | Enterprise / Self-hosted | Medium |
| 🟡 P2 | Matrix | Open source | Medium-High |
| 🟡 P2 | LINE | Japan / Taiwan / Thailand | Low |
| 🟡 P2 | IRC | DevOps community | Low |
| 🟢 P3 | Feishu / Lark | China / SEA enterprise | Medium |
| 🟢 P3 | Signal | Security-conscious | Medium |
| 🟢 P3 | Zalo | Vietnam | Low |
| 🟢 P3 | DingTalk | China enterprise | Low-Medium |
| 🟢 P3 | BlueBubbles | Apple ecosystem | Low |
| 🟢 P3 | Nextcloud Talk | EU / Government | Low |
| 🟢 P3 | Twitch | Streaming / DevRel | Low |
| ⚪ P4 | Nostr | Decentralized | Medium |
| ⚪ P4 | Tlon (Urbit) | Niche | High |
| ⚪ P4 | Synology Chat | NAS users | Low |
| ⚪ P4 | Webchat UI | Universal | Low |

---

## Architecture Notes

All channel adapters follow the same pattern:
1. Implement `ChannelAdapter` trait (`kind()` + `start()`)
2. Connect to the gateway via internal WebSocket
3. Receive platform messages → normalize to `InboundMessage::Chat`
4. Receive `OutboundMessage::Reply` from gateway → send via platform API
5. Handle platform-specific formatting (Markdown variants, message limits)

Webhook-based platforms (WhatsApp, Teams, Google Chat, LINE) also need:
- A route added to `mclaw-gateway/src/server.rs`
- Payload parsing in the webhook handler
- Webhook verification endpoint (if required by the platform)

Adding a new channel typically requires:
- ~200-400 lines of Rust
- 1 new file in `mclaw-channels/src/`
- 1 feature flag in `mclaw-channels/Cargo.toml`
- 1 webhook route in `mclaw-gateway/src/server.rs` (if webhook-based)
- Config struct + startup code in `mclaw/src/main.rs`

---

## OpenClaw Parity Tracker

| Platform | OpenClaw | MerlionClaw | Gap |
|----------|----------|-------------|-----|
| Telegram | ✅ | ✅ | - |
| Slack | ✅ | ✅ | - |
| Discord | ✅ | ✅ | - |
| WhatsApp | ✅ | ✅ | - |
| Microsoft Teams | ✅ | ✅ | - |
| Google Chat | ✅ | ❌ | Phase 1 |
| Mattermost | ✅ | ❌ | Phase 1 |
| Matrix | ✅ | ❌ | Phase 2 |
| LINE | ✅ | ❌ | Phase 2 |
| IRC | ✅ | ❌ | Phase 2 |
| Feishu / Lark | ✅ | ❌ | Phase 3 |
| Signal | ✅ | ❌ | Phase 4 |
| BlueBubbles | ✅ | ❌ | Phase 4 |
| Twitch | ✅ | ❌ | Phase 4 |
| Nostr | ✅ | ❌ | Phase 5 |
| Tlon (Urbit) | ✅ | ❌ | Phase 5 |
| Synology Chat | ✅ | ❌ | Phase 5 |
| Zalo | ✅ | ❌ | Phase 3 |
| Nextcloud Talk | ✅ | ❌ | Phase 2 |
| Webchat | ✅ | Partial | Phase 6 |
| iMessage (legacy) | ✅ | ❌ | Deprecated in OpenClaw |
| Zalo Personal | ✅ | ❌ | Phase 3 |
