# AI-Powered Specification-Driven IDE — Project Status & Roadmap

## 1. Executive Summary

This project is an **AI-powered Integrated Development Environment (IDE)** where structured YAML specifications serve as the authoritative single source of truth for software systems. 

Unlike traditional code generators or chatbots, it features:
- Deterministic AST semantic change detection ($3.32\,\mu\text{s}/\text{op}$).
- Transitive dependency impact analysis with cycle detection ($93.28\,\mu\text{s}/\text{op}$).
- Token-bounded, task-specific AI context building.
- Non-destructive unified diff patch planning with conflict protection.
- Subprocess validation and bounded AI repair loops ($N \le 3$).
- Safe Git staging, snapshot commits, and one-click rollback.
- Specification drift detection comparing YAML contracts with source code.

---

## 2. What Has Been Completed (Working Features)

### 2.1. Rust Core Engine (`crates/ai-ide-core`)
- **Specification Engine & Schema Validation (`src/specs/`)**:
  - Maintained YAML parser using `serde_yml`.
  - JSON Schema Draft-07 / 2020-12 validator using `jsonschema` for `system`, `api`, `database`, `ui`, `business-logic`, and `security` specifications.
  - `SpecRegistry` indexing all nodes (`SpecNode`) with cross-reference validation.
- **Deterministic Semantic Diff Detector (`src/changes/`)**:
  - Traverses YAML ASTs to extract structural changes (`ChangeKind::Added`, `Modified`, `Removed`) by JSON path rather than naive line differences.
- **Dependency Graph & Impact Analysis (`src/graph/`)**:
  - Backed by `petgraph::graph::DiGraph`.
  - Detects circular dependencies (`IdeError::CircularDependency`).
  - Computes transitive impact sets: **Directly Affected**, **Transitively Affected**, and **Verification-Only (Tests)**.
- **Task Context Builder (`src/context/`)**:
  - Combines spec diffs, affected file slices, conventions, and test failures.
  - Bounded token estimator to prevent context window explosion.
- **Pluggable AI Provider Abstraction (`src/ai/`)**:
  - `AiProvider` trait (`async_trait`).
  - `MockAiProvider`: Deterministic provider for hermetic testing and offline validation.
  - `GeminiAiProvider`: Reusable `reqwest` client connecting to Google Gemini REST endpoints.
- **Patch Engine & Conflict Protection (`src/patch/`)**:
  - Generates unified diffs using the `similar` crate.
  - Detects if manual modifications occurred on disk since generation base. Never silently overwrites manual code.
- **Validation Runner & Bounded Repair Loop (`src/validation/`)**:
  - Executes subprocess test commands (e.g. `node --test`) with working directory isolation and exit code capturing.
  - `RepairOrchestrator`: Automatically captures test failure diagnostics and feeds them back into targeted repair loops up to $N = 3$ iterations.
- **Git & Rollback Engine (`src/git/`)**:
  - Inspects branch status, unified diffs, stages changes, and provides instant working tree rollback (`git checkout -- .` and `git clean -fd`).
- **SQLite Generation History (`src/storage/`)**:
  - Embedded `rusqlite` database (`.ai-ide-history.db`) saving generation runs, patches applied, timestamps, and rollback metadata.
- **Specification Drift Detector (`src/drift/`)**:
  - Scans implemented routes and source files to verify exported functions and schema properties match YAML contracts.
- **Workspace Security (`src/workspace/`)**:
  - Detects path traversal (`../`) and out-of-workspace file manipulation.
  - Debounced file watcher (`notify`).

### 2.2. Local IDE Server (`crates/ai-ide-server`)
- Built with **Tokio** and **Axum 0.8**.
- Endpoints:
  - `GET /api/workspace`: Scans workspace tree.
  - `GET /api/file` & `POST /api/file`: Reads and writes source/spec files safely.
  - `GET /api/specs` & `POST /api/specs`: Registers and validates YAML specifications.
  - `POST /api/diff`: Runs AST semantic diffing.
  - `POST /api/impact`: Runs Petgraph transitive dependency analysis.
  - `POST /api/generate`: Synthesizes implementation plans.
  - `POST /api/patch/apply`: Applies approved patches with conflict checks.
  - `POST /api/validation/run`: Executes test checks.
  - `GET /api/git/status` & `POST /api/git/rollback`: Git recovery operations.
  - `GET /api/drift`: Drift diagnostic reports.
  - `GET /api/history`: SQLite generation logs.
  - `fallback_service`: Serves frontend static production builds.

### 2.3. Automated Test Verification
- `cargo test -p ai-ide-core`: 8 unit tests passed.
- `cargo test --test vertical_slice_test`: End-to-end 12-step YAML-to-diff-to-patch pipeline test passed in $0.04\,\text{s}$.
- `cargo test --test edge_cases_test`: Conflicting manual edits, circular dependencies, and missing reference tests passed.

---

## 3. What Happened to the Frontend (Why it Needs Re-Creation)

### Root Cause
During early testing, an un-ignored `target/` build directory exhausted available drive space ($0\,\text{MB}$ remaining). When running temporary cleanup commands, the untracked `frontend/src/` files were removed from disk before being committed to Git. The `.gitignore` was configured to exclude `node_modules/` and `dist/`, leaving only empty dependency artifacts.

### Current State of `frontend/`
- The `frontend/` directory currently contains only stale cache artifacts (`.vite`, partial `.pnpm`).
- The application source code (`package.json`, `src/App.tsx`, `components/`, etc.) needs to be re-created.

---

## 4. Frontend Blueprint: Re-Creating the UI

The frontend is a lightweight **React 19 + TypeScript + Vite + Tailwind CSS + Monaco Editor** client that consumes the Rust backend at `http://127.0.0.1:3001`.

### 4.1. File Structure Needed
```text
frontend/
├── package.json               # React 19, @monaco-editor/react, lucide-react, tailwindcss
├── tsconfig.json              # Strict TypeScript configuration
├── vite.config.ts             # Vite dev server with proxy to http://127.0.0.1:3001
├── tailwind.config.js         # IDE dark theme colors (zinc/indigo)
├── postcss.config.js
├── index.html
└── src/
    ├── main.tsx
    ├── index.css
    ├── types.ts               # Matching Rust models (SpecNode, SemanticChange, FilePatch, etc.)
    ├── components/
    │   ├── layout/
    │   │   ├── Navbar.tsx     # Header: Git branch, Save, Run Tests, Scan Drift, Rollback
    │   │   └── Sidebar.tsx    # Left panel: Explorer for specs/, src/, tests/
    │   ├── editor/
    │   │   └── EditorView.tsx # Monaco Editor & Monaco Side-by-Side DiffViewer
    │   ├── assistant/
    │   │   └── AssistantPanel.tsx # Right panel: Semantic changes, Petgraph impact, Generate Plan, Approve Patches
    │   └── bottom/
    │       └── BottomPanel.tsx # Bottom panel: Test runner output, Drift alerts, History
    └── App.tsx                # Main integration orchestrating API requests & state
```

### 4.2. API Integration Mapping
- **File select**: Calls `GET /api/file?path=...`.
- **Typing in Monaco**: Calls `POST /api/diff` (semantic diffs) and `POST /api/impact` (`petgraph` impact).
- **"Generate Patch"**: Calls `POST /api/generate` (returns `ImplementationPlan`).
- **"Review Diff"**: Switches Monaco to `<DiffEditor />` comparing old vs. new code.
- **"Approve & Apply"**: Calls `POST /api/patch/apply` and triggers `POST /api/validation/run`.
- **"Rollback"**: Calls `POST /api/git/rollback` to restore the pre-generation baseline.

---

## 5. What is Missing / Next Implementation Steps

| Area | Missing Item | Description & Priority |
| :--- | :--- | :--- |
| **Frontend** | **Re-create UI & Monaco components** | Implement files detailed in Section 4 and commit to Git. **[HIGH]** |
| **Tauri Desktop** | **Tauri 2.x packaging** | Optional standalone desktop binary (`src-tauri`) once frontend is restored. **[MEDIUM]** |
| **Real AI Models** | **Anthropic / Claude & Ollama integrations** | Expand beyond Gemini and Mock providers with streaming support. **[MEDIUM]** |
| **AST Parsers** | **Native AST parsing for TS/JS** | Integrate `ts-morph` or `@swc/core` via Rust worker to analyze function signatures for deep drift detection. **[LOW]** |
| **File Watcher** | **WebSocket push notifications** | Push file change events from `notify` directly to UI over Axum WebSocket (`ws://`). **[LOW]** |

---

## 6. How to Run the Backend Right Now

```powershell
# In the repository root:
cargo run -p ai-ide-server
```
The server will start on **`http://127.0.0.1:3001`**, ready to accept API requests and serve the frontend once restored.
