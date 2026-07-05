# pi0

**A personal intelligence workbench for macOS — captures your activity, OCRs it on-device into an encrypted local store, and exposes it to AI agents over a local MCP server.**

pi0 quietly records how you use your Mac — which app is frontmost, what you type, and what's on screen — turns it into structured, searchable _context_, and exposes that context to AI agents over a local [MCP](https://modelcontextprotocol.io) server. The goal: let you (and your agents) understand and optimise how you actually spend your time and attention.

Everything stays on your machine. Screenshots are OCR'd on-device and then **deleted** — only the extracted text survives — and the whole store lives in a password-encrypted SQLite database.

> ⚠️ pi0 is **macOS-only** and requires **macOS 14 (Sonoma) or newer** (it uses ScreenCaptureKit). It needs the **Input Monitoring** and **Screen Recording** permissions to function.

---

## Quick install (pre-built)

Grab the latest pre-built app instead of building from source:

```bash
# download, unzip, and move to /Applications
curl -L -o pi0.zip https://releases.ruiqilei.com/pi0/pi0-darwin-arm64-1.0.0.zip
unzip pi0.zip
mv pi0.app /Applications/

# the build is ad-hoc signed, so clear quarantine and re-sign locally
sudo xattr -cr /Applications/pi0.app
sudo codesign --force --deep --sign - /Applications/pi0.app
```

Then launch it from `/Applications` and grant the **Input Monitoring** and **Screen Recording** permissions on first run.

> Apple Silicon (arm64) only.

## What it does

- **Keystroke capture** — records typed text per app via `IOHIDManager`, on a dedicated run-loop thread.
- **Periodic screenshots** — captures every display on a fixed interval (default 8s) using ScreenCaptureKit.
- **On-device OCR** — each screenshot is run through an embedded PP-OCRv6 model (CPU-only), producing text lines with normalised `[0,1]` screen coordinates. The image is then deleted; only text is kept.
- **Encrypted local store** — all data lands in a single SQLCipher-encrypted SQLite DB (`<dataDir>/pi0.db`), unlocked with a password you set on first launch.
- **MCP server** — a local, token-authenticated HTTP server lets other AI agents query your activity to summarise your day, break down attention, or reconstruct working context.

## How it fits together

pi0 is an Electron app with a Rust native addon:

```
┌─────────────────────────── Electron ───────────────────────────┐
│                                                                 │
│  Renderer (React 19 + Arco Design)                              │
│    password gate · permission guard · settings · tray panel     │
│                          │ IPC (preload)                        │
│  Main process (Node / TypeScript)                               │
│    settings · tray icon · MCP server (Streamable HTTP)          │
│                          │ napi                                  │
│  @pi0/native (Rust, napi-rs)                                    │
│    keylogger thread · screenshot capture · OCR thread           │
│    · encrypted SQLite store (SQLCipher)                         │
└─────────────────────────────────────────────────────────────────┘
```

- **`native/`** — the Rust addon (`@pi0/native`). Owns all the heavy, platform-specific work: HID keylogging, ScreenCaptureKit screenshots, the OCR pipeline, and the encrypted DB. Built with [napi-rs](https://napi.rs/).
- **`src/main/`** — Electron main process: settings persistence, tray, and the MCP server (`src/main/mcp/`).
- **`src/app/`, `src/renderer.tsx`, `src/panel.tsx`** — the React UI: a settings window and a small tray float panel.
- **`src/shared/`** — zod schemas shared across the main ↔ preload ↔ renderer boundary (and used to validate what the Rust addon returns).

## MCP server

The MCP server runs on `http://127.0.0.1:<mcpPort>/mcp` (default port **31415**), stateless Streamable HTTP, and requires an `Authorization: Bearer <token>` header. The token is minted once and stored inside the encrypted DB. It exposes three tools, designed to be called in order:

| Tool           | Purpose                                                                                                                                |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `apps`         | List apps used in a time range, with activity counts. **Call first** to scope analysis.                                                |
| `app-guidance` | How to read a given app's screen text — what to focus on, what to ignore (e.g. Feishu/Lark → extract recent messages, skip UI chrome). |
| `contexts`     | The OCR'd screen text itself, paginated, optionally filtered to one app.                                                               |

Settings ▸ MCP has **Copy Token** and **Copy for Agents** (a paste-ready `mcpServers` JSON block) to wire an agent up quickly.

## Privacy & security

- All processing is **local**. No network calls to third parties; screenshots never leave the device and are deleted right after OCR.
- The data store is **encrypted at rest** (SQLCipher) with a password only you know — **it cannot be recovered if forgotten.**
- The MCP server binds to **loopback only** and rejects any request without the correct bearer token.

## Requirements

- macOS 14+
- [Node.js](https://nodejs.org/) (with npm)
- A [Rust toolchain](https://rustup.rs/) (to build the native addon)
- The first native build downloads prebuilt MNN static libs and the OCR models, so it needs network access.

## Getting started

```bash
# install JS deps (this also links the local @pi0/native package)
npm install

# build the Rust native addon
npm run build:native

# run the app in development
npm start
```

On first launch you'll be asked to **create a password** (used to encrypt the store) and to **grant Input Monitoring and Screen Recording** permissions.

### Packaging

```bash
npm run make      # build a distributable (macOS .zip via electron-forge)
npm run package   # package the app without making installers
```

The forge `generateAssets` hook runs `npm run build:native` before webpack bundles the main process, so packaging builds the addon for you.

## Scripts

| Command                | Description                                  |
| ---------------------- | -------------------------------------------- |
| `npm start`            | Run the app in development (electron-forge). |
| `npm run make`         | Build distributables.                        |
| `npm run package`      | Package the app.                             |
| `npm run build:native` | Build the Rust native addon (`native/`).     |
| `npm run lint`         | ESLint over `.ts` / `.tsx`.                  |
| `npm run format`       | Prettier write.                              |
| `npm run format:check` | Prettier check.                              |

## Project layout

```
native/              Rust native addon (@pi0/native)
  src/               keylogger · capture · ocr · db · app monitor …
  models/            embedded OCR charset (models fetched on build)
src/
  index.ts           Electron main entry (window, tray, capture, dock sync)
  main/
    settings.ts      settings persistence (zod-validated)
    trayIcon.ts      tray icon
    mcp/             MCP server, auth, and app-analysis guidance
  app/               React views: App, PasswordGate, PermissionGuard,
                     SettingsView, FloatPanel
  shared/            zod schemas + IPC contract
  preload.ts         contextBridge surface (window.pi0.*)
  renderer.tsx       main window entry
  panel.tsx          tray float-panel entry
  arco-compat.ts     Arco Design ⇄ React 19 shim (imported first)
forge.config.ts      electron-forge config (webpack, fuses, makers)
```

## License

MIT © Ruiqi Lei
