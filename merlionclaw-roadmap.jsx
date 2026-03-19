import { useState } from "react";

const phases = [
  {
    id: "p0",
    phase: "Phase 0",
    title: "Foundation",
    timeline: "Week 1–3",
    color: "#0ea5e9",
    items: [
      {
        name: "mclaw CLI scaffold",
        desc: "clap + tokio async runtime, config loader (TOML), tracing/logging",
        crates: ["clap", "tokio", "serde", "toml", "tracing", "tracing-subscriber"],
      },
      {
        name: "Gateway core",
        desc: "WebSocket server (control plane), typed JSON protocol with serde, session management",
        crates: ["axum", "tokio-tungstenite", "serde_json", "uuid"],
      },
      {
        name: "LLM abstraction layer",
        desc: "Anthropic + OpenAI API client, streaming response handler, tool/function calling protocol",
        crates: ["reqwest", "futures-core", "async-stream"],
      },
      {
        name: "Skill engine v1",
        desc: "SKILL.md parser (OpenClaw compatible), skill discovery & registration, tool dispatch loop",
        crates: ["pulldown-cmark", "serde_yaml"],
      },
    ],
  },
  {
    id: "p1",
    phase: "Phase 1",
    title: "Channels & Memory",
    timeline: "Week 4–6",
    color: "#8b5cf6",
    items: [
      {
        name: "Telegram adapter",
        desc: "Bot API via teloxide, inline keyboards, file/image handling, DM pairing & allowlist",
        crates: ["teloxide", "teloxide-macros"],
      },
      {
        name: "Slack adapter",
        desc: "Bolt-style event subscription, Socket Mode, slash commands, thread support",
        crates: ["slack-morphism", "axum (webhooks)"],
      },
      {
        name: "Memory system",
        desc: "Markdown-based persistent memory (OpenClaw compatible), daily diary, long-term facts, hybrid search (keyword + semantic via local embeddings)",
        crates: ["tantivy", "fastembed-rs"],
      },
      {
        name: "Permission engine",
        desc: "Capability-based: each skill declares required permissions (fs, net, k8s, exec). Runtime enforcement before tool execution",
        crates: ["custom"],
      },
    ],
  },
  {
    id: "p2",
    phase: "Phase 2",
    title: "DevOps Skills",
    timeline: "Week 7–10",
    color: "#f59e0b",
    items: [
      {
        name: "K8s skill",
        desc: "Pod/Deployment/Service CRUD, log streaming, exec into pods, namespace management, context switching",
        crates: ["kube", "kube-runtime", "k8s-openapi"],
      },
      {
        name: "Helm skill",
        desc: "List/install/upgrade/rollback releases, values diff, chart search, dependency management",
        crates: ["kube (CRD for Helm)", "tokio::process (helm CLI wrapper)"],
      },
      {
        name: "Istio skill",
        desc: "VirtualService/DestinationRule/Gateway CRUD, traffic shifting, fault injection, mTLS status check",
        crates: ["kube (custom CRD types)"],
      },
      {
        name: "Loki/Grafana skill",
        desc: "LogQL query execution, time-range log retrieval, dashboard link generation, alert rule management via Grafana HTTP API",
        crates: ["reqwest", "chrono"],
      },
    ],
  },
  {
    id: "p3",
    phase: "Phase 3",
    title: "Advanced & WASM",
    timeline: "Week 11–14",
    color: "#ef4444",
    items: [
      {
        name: "Incident response",
        desc: "PagerDuty/OpsGenie webhook intake, auto-triage via LLM, runbook execution, status page updates, Slack war-room creation",
        crates: ["axum (webhook receiver)", "reqwest"],
      },
      {
        name: "WASM skill runtime",
        desc: "wasmtime-based sandbox for deterministic skill execution, WASI capabilities (fs, net) gated by permission engine",
        crates: ["wasmtime", "wasmtime-wasi"],
      },
      {
        name: "MCP bridge",
        desc: "MCP client implementation, reuse OpenClaw ecosystem MCP servers, SSE transport support",
        crates: ["reqwest", "eventsource-client"],
      },
      {
        name: "Terraform skill",
        desc: "Plan/apply/destroy with approval gates, state inspection, drift detection, cost estimation integration",
        crates: ["tokio::process", "serde_json (tfstate parsing)"],
      },
    ],
  },
];

const archComponents = [
  { id: "cli", label: "mclaw CLI", x: 50, y: 20, w: 120, h: 44, color: "#64748b" },
  { id: "web", label: "WebChat UI", x: 200, y: 20, w: 120, h: 44, color: "#64748b" },
  { id: "tg", label: "Telegram", x: 350, y: 20, w: 100, h: 44, color: "#0ea5e9" },
  { id: "slack", label: "Slack", x: 470, y: 20, w: 100, h: 44, color: "#8b5cf6" },
  { id: "gw", label: "Gateway (axum + WS)", x: 140, y: 110, w: 300, h: 50, color: "#f59e0b", big: true },
  { id: "agent", label: "Agent Loop", x: 60, y: 210, w: 160, h: 44, color: "#ef4444" },
  { id: "perm", label: "Permission Engine", x: 260, y: 210, w: 170, h: 44, color: "#ef4444" },
  { id: "mem", label: "Memory (tantivy)", x: 460, y: 210, w: 160, h: 44, color: "#ef4444" },
  { id: "llm", label: "LLM Provider", x: 60, y: 300, w: 140, h: 44, color: "#10b981" },
  { id: "skills_md", label: "SKILL.md Skills", x: 230, y: 300, w: 150, h: 44, color: "#f59e0b" },
  { id: "skills_wasm", label: "WASM Skills", x: 410, y: 300, w: 130, h: 44, color: "#f59e0b" },
  { id: "mcp", label: "MCP Bridge", x: 560, y: 300, w: 120, h: 44, color: "#10b981" },
  { id: "k8s", label: "K8s / Helm / Istio", x: 60, y: 390, w: 170, h: 44, color: "#0ea5e9" },
  { id: "loki", label: "Loki / Grafana", x: 260, y: 390, w: 150, h: 44, color: "#0ea5e9" },
  { id: "incident", label: "Incident Resp.", x: 440, y: 390, w: 140, h: 44, color: "#0ea5e9" },
  { id: "tf", label: "Terraform", x: 600, y: 390, w: 110, h: 44, color: "#0ea5e9" },
];

const vsData = [
  { feature: "语言", mc: "Rust", oc: "TypeScript" },
  { feature: "二进制大小", mc: "~15MB single binary", oc: "~300MB+ (Node.js runtime)" },
  { feature: "内存占用 (idle)", mc: "<10MB", oc: "~300MB base" },
  { feature: "Skill 模型", mc: "SKILL.md + WASM sandbox", oc: "SKILL.md only" },
  { feature: "权限模型", mc: "Capability-based, 编译期声明", oc: "Tool policies, runtime config" },
  { feature: "部署方式", mc: "单二进制 / K8s CRD / sidecar", oc: "Node.js + npm install" },
  { feature: "安全沙箱", mc: "WASI (memory safe)", oc: "Docker sandbox" },
  { feature: "DevOps 原生", mc: "一等公民 (kube-rs)", oc: "通过 skill 扩展" },
  { feature: "MCP 兼容", mc: "✓ (client)", oc: "✓ (server + client)" },
  { feature: "目标场景", mc: "Infra ops → general", oc: "Personal assistant" },
];

export default function MerlionClawRoadmap() {
  const [activePhase, setActivePhase] = useState("p0");
  const [activeTab, setActiveTab] = useState("roadmap");

  const currentPhase = phases.find((p) => p.id === activePhase);

  return (
    <div style={{
      minHeight: "100vh",
      background: "#0a0e17",
      color: "#e2e8f0",
      fontFamily: "'JetBrains Mono', 'SF Mono', 'Fira Code', monospace",
      padding: "24px",
    }}>
      {/* Header */}
      <div style={{ textAlign: "center", marginBottom: 32 }}>
        <div style={{
          fontSize: 13,
          letterSpacing: 6,
          color: "#0ea5e9",
          textTransform: "uppercase",
          marginBottom: 8,
        }}>
          Project Blueprint
        </div>
        <h1 style={{
          fontSize: 36,
          fontWeight: 800,
          margin: 0,
          background: "linear-gradient(135deg, #0ea5e9, #8b5cf6, #f59e0b)",
          WebkitBackgroundClip: "text",
          WebkitTextFillColor: "transparent",
          letterSpacing: -1,
        }}>
          🦁 MerlionClaw
        </h1>
        <div style={{ fontSize: 13, color: "#64748b", marginTop: 6 }}>
          Infrastructure Agent Runtime · Rust · WASM · K8s-native
        </div>
      </div>

      {/* Tab Nav */}
      <div style={{
        display: "flex",
        justifyContent: "center",
        gap: 4,
        marginBottom: 28,
      }}>
        {[
          { id: "roadmap", label: "Roadmap" },
          { id: "arch", label: "Architecture" },
          { id: "vs", label: "vs OpenClaw" },
        ].map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            style={{
              padding: "8px 20px",
              fontSize: 12,
              fontFamily: "inherit",
              border: "1px solid",
              borderColor: activeTab === tab.id ? "#0ea5e9" : "#1e293b",
              background: activeTab === tab.id ? "#0ea5e920" : "transparent",
              color: activeTab === tab.id ? "#0ea5e9" : "#64748b",
              borderRadius: 6,
              cursor: "pointer",
              transition: "all 0.2s",
              letterSpacing: 1,
              textTransform: "uppercase",
            }}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* Roadmap Tab */}
      {activeTab === "roadmap" && (
        <div>
          {/* Phase Selector */}
          <div style={{
            display: "flex",
            justifyContent: "center",
            gap: 8,
            marginBottom: 24,
            flexWrap: "wrap",
          }}>
            {phases.map((p) => (
              <button
                key={p.id}
                onClick={() => setActivePhase(p.id)}
                style={{
                  padding: "10px 16px",
                  fontSize: 12,
                  fontFamily: "inherit",
                  border: `2px solid ${activePhase === p.id ? p.color : "#1e293b"}`,
                  background: activePhase === p.id ? `${p.color}15` : "#0f1420",
                  color: activePhase === p.id ? p.color : "#475569",
                  borderRadius: 8,
                  cursor: "pointer",
                  transition: "all 0.25s",
                  minWidth: 140,
                  textAlign: "left",
                }}
              >
                <div style={{ fontWeight: 700, fontSize: 11, opacity: 0.6 }}>{p.phase}</div>
                <div style={{ fontWeight: 600, marginTop: 2 }}>{p.title}</div>
                <div style={{ fontSize: 10, opacity: 0.5, marginTop: 2 }}>{p.timeline}</div>
              </button>
            ))}
          </div>

          {/* Phase Detail */}
          {currentPhase && (
            <div style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))",
              gap: 12,
              maxWidth: 900,
              margin: "0 auto",
            }}>
              {currentPhase.items.map((item, i) => (
                <div
                  key={i}
                  style={{
                    background: "#111827",
                    border: `1px solid ${currentPhase.color}30`,
                    borderRadius: 10,
                    padding: 18,
                    transition: "all 0.2s",
                    borderLeft: `3px solid ${currentPhase.color}`,
                  }}
                >
                  <div style={{
                    fontSize: 14,
                    fontWeight: 700,
                    color: currentPhase.color,
                    marginBottom: 8,
                  }}>
                    {item.name}
                  </div>
                  <div style={{ fontSize: 12, color: "#94a3b8", lineHeight: 1.6, marginBottom: 12 }}>
                    {item.desc}
                  </div>
                  <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
                    {item.crates.map((c, j) => (
                      <span
                        key={j}
                        style={{
                          fontSize: 10,
                          padding: "2px 8px",
                          background: `${currentPhase.color}15`,
                          color: currentPhase.color,
                          borderRadius: 4,
                          border: `1px solid ${currentPhase.color}30`,
                        }}
                      >
                        {c}
                      </span>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Architecture Tab */}
      {activeTab === "arch" && (
        <div style={{ maxWidth: 780, margin: "0 auto" }}>
          <svg viewBox="0 0 740 460" style={{ width: "100%" }}>
            {/* Connection lines */}
            {[
              // channels → gateway
              [110, 64, 220, 110], [260, 64, 290, 110], [400, 64, 320, 110],
              [520, 64, 370, 110],
              // gateway → agent layer
              [200, 160, 140, 210], [290, 160, 345, 210], [400, 160, 540, 210],
              // agent → skills
              [140, 254, 140, 300], [140, 254, 305, 300], [345, 254, 475, 300],
              [540, 254, 620, 300],
              // skills → infra
              [305, 344, 145, 390], [305, 344, 335, 390], [475, 344, 510, 390],
              [620, 344, 655, 390],
            ].map(([x1, y1, x2, y2], i) => (
              <line key={i} x1={x1} y1={y1} x2={x2} y2={y2}
                stroke="#1e293b" strokeWidth={1.5} strokeDasharray="4,3" />
            ))}

            {/* Layer labels */}
            {[
              [8, 44, "Channels"],
              [8, 134, "Core"],
              [8, 234, "Engine"],
              [8, 324, "Skills"],
              [8, 414, "Infra"],
            ].map(([x, y, label], i) => (
              <text key={i} x={x} y={y} fill="#334155" fontSize={9}
                fontFamily="inherit" fontWeight={700} textAnchor="start">
                {label}
              </text>
            ))}

            {/* Components */}
            {archComponents.map((c) => (
              <g key={c.id}>
                <rect
                  x={c.x} y={c.y} width={c.w} height={c.h}
                  rx={6}
                  fill={`${c.color}12`}
                  stroke={c.color}
                  strokeWidth={c.big ? 2 : 1.2}
                />
                <text
                  x={c.x + c.w / 2} y={c.y + c.h / 2 + 1}
                  fill={c.color}
                  fontSize={c.big ? 12 : 11}
                  fontFamily="inherit"
                  fontWeight={600}
                  textAnchor="middle"
                  dominantBaseline="middle"
                >
                  {c.label}
                </text>
              </g>
            ))}
          </svg>

          <div style={{
            textAlign: "center",
            fontSize: 11,
            color: "#475569",
            marginTop: 8,
          }}>
            axum WebSocket gateway → agent loop → skill dispatch → infrastructure APIs
          </div>
        </div>
      )}

      {/* VS Tab */}
      {activeTab === "vs" && (
        <div style={{ maxWidth: 720, margin: "0 auto", overflowX: "auto" }}>
          <table style={{
            width: "100%",
            borderCollapse: "separate",
            borderSpacing: 0,
            fontSize: 12,
          }}>
            <thead>
              <tr>
                <th style={{ ...thStyle, borderTopLeftRadius: 8 }}>Feature</th>
                <th style={{ ...thStyle, color: "#0ea5e9" }}>
                  🦁 MerlionClaw
                </th>
                <th style={{ ...thStyle, borderTopRightRadius: 8, color: "#ef4444" }}>
                  🦞 OpenClaw
                </th>
              </tr>
            </thead>
            <tbody>
              {vsData.map((row, i) => (
                <tr key={i}>
                  <td style={{
                    ...tdStyle,
                    color: "#94a3b8",
                    fontWeight: 600,
                    borderBottomLeftRadius: i === vsData.length - 1 ? 8 : 0,
                  }}>
                    {row.feature}
                  </td>
                  <td style={{ ...tdStyle, color: "#e2e8f0" }}>{row.mc}</td>
                  <td style={{
                    ...tdStyle,
                    color: "#64748b",
                    borderBottomRightRadius: i === vsData.length - 1 ? 8 : 0,
                  }}>
                    {row.oc}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Footer */}
      <div style={{
        textAlign: "center",
        marginTop: 40,
        padding: "20px 0",
        borderTop: "1px solid #1e293b",
        fontSize: 11,
        color: "#334155",
      }}>
        <code>cargo init merlionclaw && cargo add tokio axum kube clap serde</code>
      </div>
    </div>
  );
}

const thStyle = {
  padding: "12px 16px",
  textAlign: "left",
  background: "#111827",
  borderBottom: "2px solid #1e293b",
  fontSize: 12,
  fontFamily: "inherit",
  letterSpacing: 1,
  textTransform: "uppercase",
};

const tdStyle = {
  padding: "10px 16px",
  borderBottom: "1px solid #1e293b15",
  background: "#0a0e17",
  fontFamily: "inherit",
  lineHeight: 1.5,
};
