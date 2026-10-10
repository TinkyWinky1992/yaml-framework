# AI Spec IDE

An IDE where YAML specs under `specs/` are the source of truth. An AI turns spec changes into
reviewed code patches, and the IDE checks them with tests and drift detection.

## 1. What it does

- **Semantic diff.** Compares the old and new spec structurally (by JSON path), not line by line.
- **Impact analysis.** A petgraph dependency graph shows which code, and which tests, a spec change touches.
- **Patch planning.** The AI returns full file contents. The IDE computes the unified diffs, so they are always correct.
- **Conflict protection.** An approved patch is refused if the file was edited by hand after generation. Manual edits are never overwritten.
- **Validation and repair.** Test commands run after Approve. On failure, **Try to fix (max 3)** sends the output back to the AI.
- **Rollback.** Undoes the last applied AI change from the patch history in `.ai-ide-history.db`. Files edited since are left alone. The IDE never runs git.
- **Drift detection.** Checks that exported functions and response fields match the YAML contracts.
- **Live updates.** File changes on disk are pushed to the UI over `/ws`.

## 2. Repository layout

| Path | What it is |
| :--- | :--- |
| `crates/ai-ide-core` | Rust library: specs, semantic diff, graph, context, AI providers, patch engine, validation and repair, history, drift, file watching, workspace I/O. |
| `crates/ai-ide-server` | Axum 0.8 HTTP and WebSocket server. Serves `/api/*`, `/ws`, and the built frontend from `frontend/dist`. |
| `frontend/` | React 19, TypeScript, Vite 6, Tailwind 3.4, Monaco (bundled locally). Managed with pnpm. |
| `specs/`, `src/`, `tests/` | Sample project the IDE operates on. |

## 3. Running it

### Prerequisites
- Rust (stable) and Cargo
- Node.js 22+ and pnpm (for the frontend, and for the sample `node --test` command)

### Backend
```powershell
# From the repository root
cargo run -p ai-ide-server
```
The server listens on `http://127.0.0.1:3001` and serves the built frontend from `frontend/dist`.
If the port is taken, it exits with a message instead of panicking. Stop the old instance with
`Get-Process ai-ide-server | Stop-Process`, or pick another port (see section 4).

### Frontend (development)
```powershell
pnpm --dir frontend install
pnpm --dir frontend dev
```
Open `http://localhost:5173`. Vite proxies `/api` and `/ws` to the backend. If you change the
backend port, set the same `AI_IDE_PORT` for both processes.

### Frontend (production build)
```powershell
pnpm --dir frontend build
```
The output goes to `frontend/dist`, which the server serves at `http://127.0.0.1:3001`.

## 4. Configuration (environment variables)

| Variable | Default | Purpose |
| :--- | :--- | :--- |
| `AI_IDE_HOST` | `127.0.0.1` | Address the server binds to. |
| `AI_IDE_PORT` | `3001` | Port the server binds to. The Vite dev proxy uses the same value. |
| `ANTHROPIC_API_KEY` | (unset) | Enables the **Claude (Anthropic)** provider. Never logged. |
| `ANTHROPIC_MODEL` | `claude-sonnet-5-5` | Claude model to use. |
| `OLLAMA_HOST` | `http://127.0.0.1:11434` | Ollama endpoint. The provider is available when `GET /api/tags` responds. |
| `OLLAMA_MODEL` | `llama3.1` | Ollama model to use. |
| `GEMINI_API_KEY` | (unset) | Enables the **Gemini** provider. |
| `GEMINI_MODEL` | `gemini-2.5-flash` | Gemini model to use. |

Keys are read from environment variables. They are not stored in the workspace or in the frontend.
(Storing keys in the OS keychain is planned, see section 8.)

The active provider is picked in the navbar. It starts as Claude if `ANTHROPIC_API_KEY` is set,
then Gemini if `GEMINI_API_KEY` is set, and otherwise **Mock (offline)**, a deterministic
template provider that needs no network. Providers that are not configured are shown disabled.

Example:
```powershell
$env:ANTHROPIC_API_KEY = "sk-ant-..."
cargo run -p ai-ide-server
```

## 5. HTTP and WebSocket API

| Method and path | Purpose |
| :--- | :--- |
| `GET /api/workspace` | List spec, `src/` and `tests/` files. |
| `GET /api/file?path=` · `POST /api/file` | Read and write a file. |
| `GET /api/specs` · `POST /api/specs` | List specs. Save and validate a spec. |
| `POST /api/diff` | Semantic diff of a spec against its saved version. |
| `POST /api/impact` | Directly affected, transitively affected, and verification-only files. |
| `POST /api/generate` | Ask the active provider for an `ImplementationPlan`. |
| `POST /api/repair` | Run checks and, while they fail, send the output back to the provider (max 3). Applied patches are recorded for rollback. |
| `GET /api/providers` · `POST /api/providers {id}` | List providers and their availability. Switch the active provider. |
| `POST /api/patch/apply` | Apply approved patches, with conflict checks. |
| `POST /api/validation/run` | Run test commands. |
| `POST /api/rollback` | Undo the last applied AI change. |
| `GET /api/drift` | Drift diagnostics. |
| `GET /api/history` | Generation history from SQLite. |
| `GET /ws` | Pushes `{"kind": "created"\|"modified"\|"removed", "paths": [...]}` for changes under the workspace. Ignores `target/`, `node_modules/`, `dist/`, `.git/` and the history database. Events are debounced by 200 ms. |

## 6. Tests

```powershell
cargo test --workspace                      # all tests
cargo test --test edge_cases_test           # edge cases: conflicts, rollback refusal, cycles, missing refs, path traversal
cargo test --test vertical_slice_test       # end-to-end: spec change to diff to patch
cargo check --workspace --all-targets       # must report zero warnings
```

Current counts: 38 tests across `ai-ide-core` (32 unit, 5 edge cases, 1 vertical slice). The server
has no tests of its own yet. Provider response parsing is unit-tested with canned JSON, so no test
makes a network call.

## 7. Known limits

- The AI providers have been checked against canned responses only. A real Claude or Ollama
  run is still needed to confirm the prompt works end to end.
- Drift detection still uses text matching. Replacing it with AST parsing is planned.
- The desktop app (Tauri) is not built yet. Today the IDE runs as a server plus a browser tab.

## 8. Roadmap

The full plan is in [docs/NEXT_STEPS_PROMPT.md](docs/NEXT_STEPS_PROMPT.md). Still open:

- **Desktop app.** Tauri 2 packaging with an embedded server, Open Folder, and an installer.
- **Polish.** Command palette, tabs, explorer actions, settings page, problems panel, status bar, and more.
- **Drift.** AST-based TypeScript analysis with swc, instead of text matching.
- **Secrets.** Store API keys in the OS keychain.
