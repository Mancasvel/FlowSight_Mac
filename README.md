# FlowSight (macOS)

**Privacy-first developer productivity intelligence — runs locally on your Mac.**

This repository is the **macOS** edition of FlowSight (Apple Silicon / Intel).  
The Windows edition lives in a separate repository and is not modified from here.

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](./LICENSE)

FlowSight is a desktop application that helps distributed engineering teams understand how their work flows, without the surveillance baggage of traditional productivity tools. **All sensitive processing happens on the developer's machine.**

---

## Features

- **100% local inference** — bundled `llama.cpp` (Metal) + Qwen3-VL GGUF.
- **Desktop-native** — Tauri 2 (Rust) shell, Vite frontend, SQLite. Ships as `.app` / `.dmg`.
- **Activity-oriented** — Accessibility + frontmost-app signals (not keystroke surveillance).
- **Team analytics, with consent** — opt-in aggregation into Supabase only when joining a team.

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

## Build installer

```bash
pnpm build
```

Output: `apps/agent/src-tauri/target/release/bundle/dmg/` and `.../macos/`.

## Releases / CI

Publishing a GitHub Release tag (e.g. `v3.6.0`) runs `.github/workflows/release.yml`, which:

1. Builds `llama-server` (Metal)
2. Fetches GGUF weights
3. Compiles the Tauri app and packages a `.dmg`
4. Attaches the installer to the release

Push/PR to `main` runs `.github/workflows/ci.yml` (`cargo check` + tests).

## Permissions (first run)

Grant when macOS prompts:

- **Screen Recording** — local vision summaries
- **Accessibility** — frontmost app / UI focus signals

## Support

- Commercial: manuel@flowsight.site
- Ko-fi: https://ko-fi.com/flowsight
