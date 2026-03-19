# TASK-010: Slack Channel Adapter

## Objective
Implement a Slack bot adapter using Socket Mode for real-time messaging, supporting threads, slash commands, and rich message formatting.

## Dependencies
- TASK-002 must be complete (gateway)
- TASK-005 recommended (Telegram adapter as reference)

## Steps

### 1. Slack app setup requirements

The user needs to create a Slack app at api.slack.com with:
- **Socket Mode** enabled (no public URL needed)
- **Bot Token Scopes**: `chat:write`, `app_mentions:read`, `im:history`, `im:read`, `im:write`, `channels:history`, `groups:history`
- **Event Subscriptions** (via Socket Mode): `message.im`, `app_mention`, `message.channels` (optional)
- **Slash commands** (optional): `/mclaw`
- Two tokens: `SLACK_APP_TOKEN` (xapp-...) and `SLACK_BOT_TOKEN` (xoxb-...)

### 2. Implement Slack adapter (mclaw-channels/src/slack.rs)

```rust
pub struct SlackAdapter {
    config: SlackConfig,
}

pub struct SlackConfig {
    pub app_token: String,     // xapp-... for Socket Mode
    pub bot_token: String,     // xoxb-... for API calls
    pub allow_from: Vec<String>,  // allowed user IDs or channel IDs
    pub channels: Vec<String>,     // channels to listen in (require @mention)
    pub require_mention: bool,     // in channels, only respond when @mentioned
}
```

Use `slack-morphism` crate for:
- Socket Mode connection (WSS)
- Event parsing
- API calls (chat.postMessage, reactions.add)

### 3. Message handling

#### DM messages
- Receive `message.im` event
- Check `allow_from` (user ID)
- Forward to gateway as `InboundMessage::Chat`

#### Channel messages
- Receive `message.channels` or `app_mention` event
- If `require_mention` is true, only process messages that @mention the bot
- Strip the `<@BOT_USER_ID>` from the message text before sending to agent
- Reply in a **thread** (set `thread_ts` to the original message's `ts`)

#### Slash commands
- Receive `/mclaw <text>` command
- Process `<text>` as a regular chat message
- Respond as ephemeral message (only visible to the command sender)

### 4. Response formatting

Slack uses mrkdwn (not standard Markdown). Convert agent responses:

```rust
fn to_slack_mrkdwn(text: &str) -> String {
    // Convert:
    // **bold** → *bold*
    // `code` → `code` (same)
    // ```code block``` → ```code block``` (same)
    // [link](url) → <url|link>
    // - list item → • list item
    // > blockquote → > blockquote (same)
    // Tables → preformatted text block
}
```

For structured data (pod lists, helm releases), use Slack Block Kit:
```json
{
  "blocks": [
    {
      "type": "section",
      "text": { "type": "mrkdwn", "text": "*Pods in default namespace:*" }
    },
    {
      "type": "section",
      "text": { "type": "mrkdwn", "text": "```\nNAME              STATUS    RESTARTS   AGE\nnginx-abc123      Running   0          3d\n```" }
    }
  ]
}
```

### 5. Long-running operation UX

For operations that take >3 seconds:
1. Immediately send a "thinking" reaction (`:hourglass:`) to the user's message
2. When done, remove the reaction and send the reply
3. If streaming is available, send initial message then update it with `chat.update`

### 6. Approval flow integration

When the permission engine requires approval (TASK-008):
1. Send an interactive message with buttons:
   ```
   ⚠️ Approval required: `helm upgrade nginx` needs `exec:helm`.
   [Approve] [Deny]
   ```
2. Listen for `block_actions` event
3. Route approval/denial back to the agent

### 7. Wire into CLI

Update startup flow:
- If `channels.slack.enabled = true`, start SlackAdapter alongside TelegramAdapter
- Both adapters connect to the same gateway

## Validation

```bash
cargo test -p mclaw-channels -- slack

# Integration test:
# 1. DM the bot: "list pods in default"
#    → Bot replies with formatted pod list
# 2. In a channel, @mention: "@mclaw show helm releases"
#    → Bot replies in thread with release list
# 3. Slash command: /mclaw status
#    → Ephemeral response with gateway status
# 4. Approval flow: "delete the nginx deployment"
#    → Bot sends approval buttons
#    → Click Approve → deployment deleted
```

## Output

A working Slack bot adapter with DM support, channel @mentions with threaded replies, slash commands, Slack Block Kit formatting, and interactive approval buttons.
