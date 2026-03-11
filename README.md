# IBEX Desktop

Native macOS application for [IBEX](https://github.com/Percona-Lab/IBEX) — a workplace AI assistant that connects to your team's Slack, Jira, Notion, and more.

Built with [Tauri 2](https://tauri.app/) (Rust + Svelte), wrapping [Open WebUI](https://github.com/open-webui/open-webui) in a native window with automatic setup, process management, and a menu bar status indicator.

**Status: Early Development**

---

## Install

### Prerequisites

| Requirement | Details |
|---|---|
| **macOS** | 10.13 (High Sierra) or later — Apple Silicon and Intel both supported |
| **Docker Desktop** | [Download here](https://www.docker.com/products/docker-desktop/) — must be installed and **running** before first launch |
| **Disk space** | ~2–3 GB free (for the Open WebUI Docker image + data) |
| **Internet** | Required for pulling the Docker image and connecting to LLM APIs |

> **Note for Percona employees:** Connecting to the Percona VPN gives access to internal Ollama instances. The setup wizard checks for this but it's optional — external LLM endpoints work without VPN.

### What's Bundled (no separate install needed)

- Node.js runtime (native ARM64 / Intel)
- MCP connector servers (Slack, Jira, Notion, ServiceNow, Salesforce, Memory)
- Open WebUI frontend
- Admin account auto-creation (credentials stored in macOS Keychain)

### Download & Install

1. Download the latest `.dmg` from [Releases](https://github.com/Percona-Lab/IBEX-desktop/releases)
2. Open the DMG and drag **IBEX.app** to Applications
3. Make sure Docker Desktop is running
4. Launch IBEX — the first-run setup wizard will walk you through configuration

### First-Run Setup

The setup wizard will:
1. **Check dependencies** — verifies Docker is running (and optionally checks Percona VPN)
2. **Configure connectors** — enter API tokens for the services you want to connect (Slack, Jira, Notion, etc.). All connectors are optional and can be configured later from Settings.
3. **Provision automatically** — creates an admin account, pulls the Open WebUI Docker image, and starts MCP servers

---

## What It Does

- Wraps Open WebUI in a native macOS window (WKWebView, not a browser)
- Auto-creates an admin account and logs in silently
- Manages Docker container and MCP server processes
- Menu bar icon with live status (green/yellow/red)
- Native settings UI for configuring connectors (Slack, Jira, Notion, etc.)
- First-run setup wizard
- Reads/writes `~/.ibex-mcp.env` (backward compatible with terminal `start.sh`)

## Architecture

```
IBEX.app
├── src-tauri/src/           # Rust backend
│   ├── lib.rs               # Tauri setup, startup sequence
│   ├── config.rs            # Read/write ~/.ibex-mcp.env
│   ├── keychain.rs          # macOS Keychain (admin creds)
│   ├── docker.rs            # Docker management (bollard)
│   ├── process.rs           # Node.js server lifecycle
│   ├── account.rs           # Auto-account creation + auth
│   ├── prompt.rs            # System prompt generation
│   ├── state.rs             # Shared AppState
│   └── tray.rs              # Menu bar icon + status
├── src/                     # Svelte frontend
│   ├── routes/settings/     # Settings window
│   ├── routes/setup/        # First-run wizard
│   └── lib/components/ibex/ # Connector forms, status
├── src-tauri/binaries/      # Bundled Node.js (per-arch)
└── src-tauri/resources/     # servers/, connectors/, node_modules/
```

## Dev Setup

### Prerequisites

- **Rust** (latest stable): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Node.js** 18+ with pnpm: `npm install -g pnpm`
- **Xcode Command Line Tools**: `xcode-select --install`

### Build & Run

```bash
git clone https://github.com/Percona-Lab/IBEX-desktop.git
cd IBEX-desktop
pnpm install
pnpm run tauri dev
```

First build takes several minutes (compiling Rust dependencies).

## Relationship to IBEX

This is the native macOS frontend for [IBEX](https://github.com/Percona-Lab/IBEX). The terminal-based IBEX (`install.sh`, `start.sh`, `configure.sh`) continues to work independently. Both share the same `~/.ibex-mcp.env` config file.

## License

MIT License. See [LICENSE](./LICENSE).

Based on [open-webui-desktop](https://github.com/reecelikesramen/open-webui-desktop) by Reece Holmdahl.
