# IBEX Desktop

Native macOS application for [IBEX](https://github.com/Percona-Lab/IBEX) — a workplace AI assistant that connects to your team's Slack, Jira, Notion, and more.

Built with [Tauri 2](https://tauri.app/) (Rust + Svelte), wrapping [Open WebUI](https://github.com/open-webui/open-webui) in a native window with automatic setup, process management, and a menu bar status indicator.

**Status: Early Development**

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
