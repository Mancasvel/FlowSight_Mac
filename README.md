# FlowSight (macOS)

**Privacy-first developer productivity intelligence — runs locally on your Mac.**

This repository is the **macOS** edition of FlowSight (Apple Silicon / Intel).  
The Windows edition lives in a separate repository and is not modified from here.

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](./LICENSE)

FlowSight is a desktop application that helps distributed engineering teams understand how their work flows, without the surveillance baggage of traditional productivity tools. **All sensitive processing happens on the developer's machine.**

---

## Features

- **100% local inference** — bundled `llama.cpp` (Metal) + quantized Qwen3-VL-2B-Instruct GGUF downloaded once and verified on first use.
- **Desktop-native** — Tauri 2 (Rust) shell, Vite frontend, SQLite. Ships as `.app` / `.dmg`.
- **Activity-oriented** — Accessibility + frontmost-app signals (not keystroke surveillance).
- **Team analytics, with consent** — opt-in aggregation into Supabase only when joining a team.

## Bring your own AI (MCP)

The installed app includes a read-only [FlowSight MCP server](docs/MCP.md).
Open Settings > Connect your AI for the exact command to use in a compatible
desktop AI client. No extra runtime is required, and activity descriptions
and ticket IDs are excluded by default. A cloud AI client may receive the
returned report data.

## Prerequisites

- macOS 12+
- Xcode / CLT (`sudo xcodebuild -license accept`)
- Rust stable, Node.js 18+, pnpm 8+

## Install and run

```bash
git clone https://github.com/Mancasvel/FlowSight_Mac.git
cd FlowSight_Mac
pnpm install
bash scripts/prepare-macos-llm.sh
pnpm dev
```

## Model weights

The `.app` ships only the `llama.cpp` runtime (code that must be signed and notarized). The ~1.55 GB of Qwen3-VL-2B-Instruct GGUF weights are **not** bundled: the app downloads them once into `~/Library/Application Support/ai.flowsight.agent/models/` and verifies their SHA-256 before use. Inference works offline afterward. See [model provenance and license](./local_llm/MODEL_NOTICE.md).

For local development you can pre-populate `local_llm/` instead, which the app prefers over downloading:

```bash
node scripts/fetch-models.mjs          # download if missing
node scripts/fetch-models.mjs --check  # verify only, no network
```

## Build installer

```bash
# Native (this Mac's chip)
pnpm build

# Explicit Apple Silicon
pnpm run build:mac:arm

# Explicit Intel (cross-compile from Apple Silicon OK)
pnpm run build:mac:intel
```

Output under `apps/agent/src-tauri/target/<triple>/release/bundle/`.

## Releases / CI

Publishing a GitHub Release tag (e.g. `v3.6.0`) runs `.github/workflows/release.yml`, which builds **two** installers from this same repo:

| Asset | Chip |
|---|---|
| `*_aarch64.dmg` | Apple Silicon (M1+) |
| `*_x64.dmg` | Intel |

Push/PR to `main` runs `.github/workflows/ci.yml` (`cargo check` + tests).

## Permissions (first run)

Grant when macOS prompts:

- **Screen Recording** — local vision summaries
- **Accessibility** — frontmost app / UI focus signals

## Support

- Commercial: manuel@flowsight.site
- [Buy me a coffee on Ko-fi](https://ko-fi.com/mancasvel)
