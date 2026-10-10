# Prompt: Finish the AI Spec IDE and ship it as a desktop app

> Paste everything below this line into a new coding-agent session opened at the repository root.

---

You are working on **AI Spec IDE**. In this IDE, YAML specs under `specs/` are the source of truth, and an AI turns spec changes into reviewed code patches. The workspace contains:

- **`crates/ai-ide-core`** (Rust library), with these modules: specs, semantic changes, a petgraph dependency graph, context builder, AI providers, the patch engine, validation runner and repair loop, SQLite history, drift detection, and workspace I/O.
- **`crates/ai-ide-server`** (Axum 0.8 on `127.0.0.1:3001`). It serves `/api/*` and the built frontend from `frontend/dist`.
- **`frontend/`** (React 19, TypeScript, Vite 6, Tailwind 3.4, and Monaco bundled locally; managed with pnpm). In dev it runs on `localhost:5173` and proxies `/api` and `/ws` to `3001`.
- **Sample project files** that the IDE operates on: `specs/`, `src/routes/users.ts`, and `tests/users.test.mjs`.

Ground rules:
- Work one step at a time. After each step, run `cargo test --workspace`, `cargo check --workspace --all-targets` (it must produce zero warnings), and `pnpm --dir frontend build`. Then tell the user what changed and give a small example of how to check it themselves.
- **The IDE must never run git commands.** The git module was removed on purpose. Rollback works from the patch history in `.ai-ide-history.db`; see `PatchEngine::check_revert` and `revert_patch` in `crates/ai-ide-core/src/patch/mod.rs`. Never run `git clean`, `git checkout -- .` or anything else that deletes user files.
- Never overwrite a file the user edited by hand. Keep the conflict checks in `PatchEngine`.
- On Windows, `ValidationRunner` passes the command line verbatim through `cmd /C` + `raw_arg`. Keep it that way.
- Match the existing code style. Don't commit unless the user asks.

---

## Step 0: Fix the startup crash (port 3001 already in use)

**Symptom**
```
> cargo run -p ai-ide-server
thread 'main' panicked at crates\ai-ide-server\src\main.rs:118:62:
called `Result::unwrap()` on an `Err` value: Os { code: 10048, kind: AddrInUse, ... }
```
`cargo build` also fails with `failed to remove file target\debug\ai-ide-server.exe`.

**Root cause.** An earlier `ai-ide-server.exe` is still running in the background and listening on `127.0.0.1:3001`. Windows locks a running .exe, which is why cargo can't replace the binary. The code makes it worse: `TcpListener::bind(addr).await.unwrap()` panics with an unreadable message instead of explaining what happened.

**Fix**
1. **Unblock immediately.** Find and stop the stale process, then confirm the port is free:
   ```powershell
   netstat -ano | findstr :3001
   Get-Process ai-ide-server | Stop-Process
   ```
2. **Make the port configurable.** In `crates/ai-ide-server/src/main.rs`, read it from `AI_IDE_PORT` (default `3001`), and the bind host from `AI_IDE_HOST` (default `127.0.0.1`).
3. **Replace the `unwrap()` on `bind` with a clear error.** For `AddrInUse`, print something like: `Port 3001 is already in use. Another AI-IDE server is probably running. Stop it (Get-Process ai-ide-server | Stop-Process) or set AI_IDE_PORT.` Then exit with code 1. Do not panic.
4. **Make the frontend use the same port.** In `frontend/vite.config.ts`, read the proxy target from `process.env.AI_IDE_PORT ?? 3001`.
5. **Add graceful shutdown** with `axum::serve(...).with_graceful_shutdown(tokio::signal::ctrl_c())` so Ctrl+C releases the port cleanly.
6. Remove the remaining `unwrap()`s on startup paths, such as `GenerationStore::in_memory().unwrap()`, and replace them with logged errors.

**How to check**
- Start the server twice. The second instance should print the friendly message and exit with code 1, without a panic.
- Run `$env:AI_IDE_PORT=3002; cargo run -p ai-ide-server`. The server should log `listening on http://127.0.0.1:3002`.

---

## Step 1: Wire up the AI providers (Claude, Ollama, Gemini, Mock)

**Current state.** `AiProvider` is defined in `crates/ai-ide-core/src/ai/provider.rs`, and `MockAiProvider` and `GeminiAiProvider` both exist. However, `generate_plan` in the server hardcodes `MockAiProvider`, and Gemini is never used. The frontend already calls `GET /api/providers` and `POST /api/providers {id}`, but those endpoints don't exist yet; see `frontend/src/api.ts` and the provider `<select>` in `Navbar.tsx`.

**Tasks**
1. Add `crates/ai-ide-core/src/ai/anthropic.rs` with an `AnthropicAiProvider` that uses `reqwest` against the Messages API (`POST https://api.anthropic.com/v1/messages`, headers `x-api-key` and `anthropic-version: 2023-06-01`).
   - Default model: `claude-sonnet-5-5`. Allow an override through `ANTHROPIC_MODEL`.
   - Read the key from `ANTHROPIC_API_KEY` and never log it.
   - Ask for a strict JSON `ImplementationPlan`. The patches should hold full `patched_content`; build `unified_diff` locally with `PatchEngine::create_diff` so diffs are always correct.
   - Validate every patch path through `WorkspaceManager::resolve_safe_path`, and reject paths outside the workspace.
2. Add `ollama.rs` with an `OllamaAiProvider` that calls `POST {OLLAMA_HOST:-http://127.0.0.1:11434}/api/chat` with `format: "json"`. Use the model from `OLLAMA_MODEL`. It counts as available only if `GET /api/tags` responds.
3. Put a `Mutex<String>` for the active provider into `AppState`. Implement the following, where `available` means the key or endpoint is configured:
   - `GET /api/providers` returns `{active, providers:[{id,label,available}]}`.
   - `POST /api/providers {id}` switches the active provider; reject providers that aren't available.
   - `generate_plan` uses the active provider.
4. Wire `RepairOrchestrator` (`validation/repair.rs`) into a new `POST /api/repair` endpoint. When tests fail after Approve, the UI offers a "Try to fix (max 3)" button that sends the failing output back to the provider.
5. Unit-test the response parsing for each provider with canned JSON. Don't make network calls in tests.

**How to check.** Set `ANTHROPIC_API_KEY`, then restart the server. The navbar dropdown should list Claude as available. Generate a patch for `phone: string`; the plan summary should come from Claude rather than from the mock text.

---

## Step 2: Push live file changes over a WebSocket (`/ws`)

**Current state.** The `notify` crate is a dependency but nothing uses it. The frontend hook `frontend/src/hooks/useFileEvents.ts` already connects to `/ws`, reconnects every 3 seconds, and expects JSON messages of the form `{kind: 'created'|'modified'|'removed', paths: string[]}` with workspace-relative paths that use forward slashes. The navbar shows Live or Offline.

**Tasks**
1. In core, add a `WorkspaceWatcher` that uses `notify` (RecommendedWatcher, recursive). It should:
   - Debounce events for about 200 ms.
   - Ignore `target/`, `node_modules/`, `dist/`, `.git/` and `.ai-ide-history.db*`.
   - Map events to created, modified or removed, with relative paths.
2. In the server, create a `tokio::sync::broadcast` channel. Add a `GET /ws` route using `axum::extract::ws::WebSocketUpgrade` that forwards events as JSON.
3. On any event under `specs/`, reload the `SpecRegistry`; on any event under `src/` or `tests/`, call `rebuild_graph()`.
4. Add a unit test for path filtering and event mapping.

**How to check.** With the IDE open, edit `src/routes/users.ts` in another editor. Within about a second, the Log tab should show `File modified: src/routes/users.ts`, the open editor should reload, and the navbar should show **Live**.

---

## Step 3: Add the missing edge-case tests and make the README accurate

1. Create `crates/ai-ide-core/tests/edge_cases_test.rs`. The README claims this file exists, but it doesn't. Cover these cases:
   - A manual edit after generation makes the patch apply fail with `PatchConflict`.
   - A rollback after a manual edit is refused, and the file stays untouched.
   - A circular spec dependency is rejected and the rejected edge isn't kept.
   - Spec cross-references that point to missing specs are reported.
   - Patch paths with `../` are rejected.
2. Rewrite the README so it matches reality:
   - Section 3 still says the frontend is missing. Replace it with how to run the frontend.
   - Update the test counts.
   - Remove the claims about a file watcher and `edge_cases_test` until they really exist.
   - Document the env vars `AI_IDE_PORT`, `ANTHROPIC_API_KEY`, `OLLAMA_HOST` and `OLLAMA_MODEL`.

**How to check:** `cargo test --test edge_cases_test` should pass.

---

## Step 4: Package it as a desktop app with Tauri 2

**Goal.** Double-click an installer, then open a native window with the IDE inside it. There should be no terminal and no `cargo run`.

**Tasks**
1. Add `src-tauri/` as a new workspace member with Tauri 2.x, and register it in the root `Cargo.toml`.
2. Run the Axum server inside the Tauri process. Move the router setup from `ai-ide-server/src/main.rs` into a reusable `pub async fn serve(root: PathBuf, listener: TcpListener)` in a library target of `ai-ide-server`. Both the CLI binary and Tauri call that function.
   - Bind to `127.0.0.1:0` (a free port chosen by the OS), so the desktop app can never hit the AddrInUse problem from Step 0.
   - Pass the chosen port to the webview. The frontend already supports `VITE_API_BASE`, or a Tauri command such as `get_api_base` can return `http://127.0.0.1:<port>`; `wsUrl()` derives the ws:// URL from it.
3. Configure `tauri.conf.json`:
   - `build.frontendDist = "../frontend/dist"`
   - `beforeBuildCommand = "pnpm --dir frontend build"`
   - `devUrl = "http://localhost:5173"`
   - Window title **AI Spec IDE**, size 1440×900, minimum 1024×640
   - App icons
   - A CSP that allows `connect-src http://127.0.0.1:* ws://127.0.0.1:*`
4. Add an **Open Folder** command using the `tauri-plugin-dialog` folder picker. It should restart the embedded server with the chosen workspace root, and remember recently opened folders in the app config dir.
5. Store the history DB in the workspace as `.ai-ide-history.db`, and store app settings (active provider, recent folders) in the Tauri app-config dir. API keys go into the OS keychain via `tauri-plugin-stronghold` or `keyring`, never in plain text.
6. Add the scripts `pnpm tauri dev` and `pnpm tauri build`. The output is a Windows `.msi` or NSIS installer.

**How to check.** `pnpm tauri dev` should open a native window showing the IDE. Use File → Open Folder to pick another project, and the explorer should switch to it. `pnpm tauri build` should produce an installer under `src-tauri/target/release/bundle/`.

---

## Step 5: Make the IDE polished for everyday use

Implement these in order, checking each one as you go:

1. **Command palette** (`Ctrl+Shift+P`) and **quick open** (`Ctrl+P`, fuzzy file search).
2. **Tabs** for multiple open files, with dirty dots, middle-click to close, and a prompt on close when there are unsaved changes.
3. **Explorer actions**: new file, new folder, rename and delete. Delete moves the file to the OS recycle bin through the `trash` crate; it never hard-deletes.
4. **Resizable panels** (sidebar, assistant, bottom). Their sizes and the last opened files persist in localStorage.
5. **Spec authoring help**: in Monaco, give YAML validation and autocomplete from the JSON Schemas in `crates/ai-ide-core/src/specs/schema.rs`. Expose them through `GET /api/schemas` and use `monaco-yaml`.
6. **Problems panel**: merge spec schema warnings, drift diagnostics and test failures, and jump to the file and line on click.
7. **Status bar**: active provider, test status, drift count, Live indicator, cursor position, and language.
8. **Settings page**: AI provider, model and keys (stored in the keychain), and the test command (instead of the hardcoded `TEST_CHECKS` in `App.tsx`). Also include the font size and the theme (light or dark; the colors are already tokens in `tailwind.config.js`).
9. **Integrated terminal tab** in the bottom panel (`xterm.js` plus a PTY via `portable-pty` over a WebSocket). Make it optional.
10. **Streaming generation**: stream the provider output into the Assistant panel so long generations don't look frozen.
11. **Startup welcome screen**: "Open Folder", recent folders, and a "Create sample project" option that copies the `specs/`, `src/` and `tests/` sample.

---

## Step 6: Replace the text-matching drift check with AST parsing

**Current state.** `crates/ai-ide-core/src/drift/detector.rs` finds the declaring file with `declares_identifier`, which is a text match. It then checks fields with `content.contains(key)`. The `phone` in the comment `// phone` would count as a match.

**Tasks.** Parse TS/JS with `swc_ecma_parser`, in pure Rust with no Node dependency. Use the parsed syntax tree for these three things:
- Find the exported function or const named after `api.name`.
- Collect the property keys of the object literals it returns and of the interface or type it is annotated with.
- Compare those keys with `responses.200.schema`. Report missing fields as Warnings and extra fields as Info, each with the exact line and column, so the Problems panel can jump to them.

Use the same parser in `graph/builder.rs` for imports instead of string scanning.

**How to check.** Add `// phone` as a comment in `users.ts`. Scan Drift should still report `phone` as missing.

---

## Definition of done

- On a clean Windows machine, installing the `.msi` and opening a folder gives a working IDE that needs no Rust or Node toolchain. The test command does need Node if the project uses it.
- The whole demo runs inside the desktop app: edit a spec, see impact, generate with Claude, review the diff, approve, see tests pass, check drift and roll back.
- `cargo test --workspace` passes, `cargo check` reports zero warnings, `pnpm --dir frontend build` succeeds, and the README matches what's actually there.
