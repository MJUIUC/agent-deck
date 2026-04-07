# AD-8.3a — File Explorer + Workspace Directories

**Story:** 8.3a — File Explorer  
**Branch:** `feature/phase8-file-explorer`  
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 8  
**Parent story:** `docs/AD-8.3.md`

---

## Summary

Agent-deck runs on a headless computer. The agent can reference file paths in chat naturally, but the user has no way to browse, navigate, or preview those files from within the app. This story adds a first-class file explorer to the thread UI: a popup modal with a tree navigator and file preview pane, triggered either by clicking a file path the agent mentions in chat or by a dedicated folder button in the thread header.

The second pillar of this story is **workspace directories**: each thread gets a persistent directory at `~/agent-deck-workspaces/{thread-id}/` that the agent knows about via its system prompt context. The agent can write files there (via MCP or terminal tools); the user can explore them. This creates a simple, filesystem-based shared context — a "stocks" thread accumulates daily reports in its workspace, the agent deeplinks to specific files in chat, and the user reads them in the explorer without leaving the conversation.

No new agent tools are introduced. The agent already outputs file paths naturally. The file explorer is a pure UI + API feature.

---

## Current State

### What already exists

- `server/src/routes/mod.rs` — Axum router; new route group is added here
- `server/src/services/tools.rs` — `AgentTool` trait and built-in tool registry (not modified by this story)
- `web/src/components/MessageBubble.tsx` — ReactMarkdown renderer with custom `pre`/`CodeBlock` override; mermaid and path deeplinks are wired in here
- `web/src/layouts/mobile/MobileChatView.tsx` — mobile thread view; explorer trigger added here
- `web/src/components/ChatView.tsx` — desktop thread view; explorer trigger added here
- `web/src/api/client.ts` — all API helpers live here; `fsApi` is added here
- `web/src/types/index.ts` — shared TypeScript types; `FsEntry` and `FsFileContent` are added here
- `Cargo.toml` — `dirs` and `base64` are already workspace dependencies

### npm packages already installed

Both were installed during initial scoping:

- `react-arborist@3.4.3` — headless virtualized tree component; fully styled by us via CSS variables
- `mermaid@11.x` — diagram renderer; called imperatively from a React component

### What does not exist yet

- `server/src/routes/fs.rs` — filesystem route handlers (new file)
- `web/src/api/client.ts` — `fsApi` block (new addition)
- `web/src/types/index.ts` — `FsEntry`, `FsFileContent` types (new additions)
- `web/src/components/FileExplorerModal.tsx` + `.module.css` — explorer UI (new files)
- Mermaid rendering in `MessageBubble.tsx` (new code block override)
- Path deeplink detection in `MessageBubble.tsx` (pre-processing pass)
- Workspace directory creation on the server
- System prompt workspace context injection in `server/src/services/context.rs`

---

## Security Model

The server validates every path before touching the filesystem:

1. Expand `~` to the resolved home directory using `dirs::home_dir()`.
2. Call `std::fs::canonicalize()` — this resolves all symlinks and `..` components and returns an error if the path does not exist.
3. Check that the canonical path starts with one of the two allowed roots:
   - `dirs::home_dir()` — the user's home directory and everything beneath it
   - `/Volumes` — macOS mount point for external drives, network shares, and removable media
4. Reject anything that does not match with a `403 Forbidden`.

This allows the user to reach mounted drives (which appear under `/Volumes` on macOS) while preventing the agent or UI from escaping `~` into system directories. Path traversal via `..` or symlink chains is defeated by `canonicalize()`.

For `read_file`:
- Files larger than 5 MB are returned as `previewable: false` with `reason: "too_large"`.
- Files that are not valid UTF-8 are checked for image extensions (`png`, `jpg`, `jpeg`, `gif`, `webp`, `svg`, `bmp`). Images are returned as base64. All other binary files return `previewable: false` with `reason: "binary"`.

---

## API Shape

### `GET /api/fs/list?path=<absolute-path>`

Returns the direct children of a directory. Used by the tree to lazy-load nodes on expand.

**Response:**
```json
{
  "data": {
    "path": "/Users/marcus/agent-deck-workspaces/abc123",
    "entries": [
      {
        "name": "report-2025-01-15.md",
        "path": "/Users/marcus/agent-deck-workspaces/abc123/report-2025-01-15.md",
        "kind": "file",
        "size": 4821,
        "modified": "2025-01-15T09:00:00Z",
        "extension": "md"
      },
      {
        "name": "charts",
        "path": "/Users/marcus/agent-deck-workspaces/abc123/charts",
        "kind": "dir",
        "size": null,
        "modified": "2025-01-15T09:00:00Z",
        "extension": null
      }
    ]
  }
}
```

Entries are sorted: directories first, then files, both groups alphabetically case-insensitively.

### `GET /api/fs/read?path=<absolute-path>`

Returns file content for preview. Only valid for files, not directories.

**Previewable text response:**
```json
{
  "data": {
    "path": "/Users/marcus/agent-deck-workspaces/abc123/report.md",
    "previewable": true,
    "content": "# Report\n...",
    "size": 4821,
    "extension": "md",
    "is_image": false
  }
}
```

**Image response:**
```json
{
  "data": {
    "path": "/Users/marcus/agent-deck-workspaces/abc123/chart.png",
    "previewable": true,
    "is_image": true,
    "image_data": "<base64>",
    "extension": "png",
    "size": 98304
  }
}
```

**Non-previewable response:**
```json
{
  "data": {
    "path": "/Users/marcus/agent-deck-workspaces/abc123/data.bin",
    "previewable": false,
    "reason": "binary",
    "size": 1048576
  }
}
```

### `GET /api/fs/workspace?thread_id=<id>`

Returns the workspace path for a thread, creating the directory if it does not exist. Used by the thread UI to know where to open the explorer by default.

**Response:**
```json
{
  "data": {
    "path": "/Users/marcus/agent-deck-workspaces/abc123"
  }
}
```

---

## Types (web/src/types/index.ts)

```ts
export interface FsEntry {
  name: string;
  path: string;
  kind: "file" | "dir";
  size: number | null;
  modified: string | null;
  extension: string | null;
}

export interface FsFileContent {
  path: string;
  previewable: boolean;
  // present when previewable === true and is_image !== true
  content?: string;
  // present when previewable === true and is_image === true
  is_image?: boolean;
  image_data?: string;
  extension?: string | null;
  size: number;
  // present when previewable === false
  reason?: "binary" | "too_large";
}
```

---

## Implementation Plan

### Task 1 — Server filesystem routes

**New file:** `server/src/routes/fs.rs`  
**Modified file:** `server/src/routes/mod.rs`

Add three async handler functions:

**`list_directory`** (`GET /api/fs/list`)
- Parse `path` query param
- Validate + canonicalize (see Security Model)
- Call `tokio::fs::read_dir`, collect entries into `FsEntry` structs
- Sort dirs-first then files, both alphabetically
- Return JSON

**`read_file`** (`GET /api/fs/read`)
- Validate + canonicalize
- Confirm it is a file, not a directory
- Check `metadata.len()` against 5 MB limit
- Attempt `tokio::fs::read`, then `String::from_utf8`
- If text: return content
- If binary + image extension: base64-encode and return
- If binary + non-image: return `previewable: false`

**`get_workspace`** (`GET /api/fs/workspace`)
- Parse `thread_id` query param
- Resolve workspace path: `dirs::home_dir() / "agent-deck-workspaces" / thread_id`
- Call `tokio::fs::create_dir_all` on the path (idempotent)
- Return the path string

In `mod.rs`, register all three routes under `/api/fs/*` within the authenticated router.

The `validate_path` helper is a private function in `fs.rs`:
```rust
fn validate_path(raw: &str) -> anyhow::Result<std::path::PathBuf> {
    // expand leading ~
    // canonicalize
    // prefix-check against home dir and /Volumes
    // return canonical PathBuf or Err
}
```

No new Cargo dependencies are needed — `dirs`, `base64`, `tokio`, `anyhow`, `serde_json`, and `chrono` are all already in the workspace.

---

### Task 2 — fsApi in client.ts and types

**Modified file:** `web/src/api/client.ts`

Add an `fsApi` block after `pushApi`:

```ts
export const fsApi = {
  list(path: string): Promise<{ data: { path: string; entries: FsEntry[] } }> {
    return apiFetch(`/api/fs/list?path=${encodeURIComponent(path)}`);
  },
  read(path: string): Promise<{ data: FsFileContent }> {
    return apiFetch(`/api/fs/read?path=${encodeURIComponent(path)}`);
  },
  workspace(threadId: string): Promise<{ data: { path: string } }> {
    return apiFetch(`/api/fs/workspace?thread_id=${encodeURIComponent(threadId)}`);
  },
};
```

Add `FsEntry` and `FsFileContent` to `web/src/types/index.ts` as shown in the Types section above.

---

### Task 3 — Mermaid rendering in markdown

**Modified file:** `web/src/components/MessageBubble.tsx`

The existing `CodeBlock` component handles all `<pre>` elements from ReactMarkdown. Extend it to detect mermaid diagrams:

- The `code` element inside a mermaid fenced block receives `className="language-mermaid"` from ReactMarkdown/remark-gfm.
- Add a `MermaidBlock` component that accepts the diagram source string, calls `mermaid.render(uniqueId, source)` inside a `useEffect`, and renders the returned SVG via `dangerouslySetInnerHTML`. Use a stable unique id per instance (e.g. `useId()` from React).
- Wire `MermaidBlock` into the ReactMarkdown `components` map as the `code` override: when `className` includes `language-mermaid`, render `<MermaidBlock>` instead of the default code element.
- Initialize mermaid once at module level: `mermaid.initialize({ startOnLoad: false, theme: 'neutral' })`. The `neutral` theme works with both light and dark app palettes without requiring custom theming.
- Handle render errors gracefully: if `mermaid.render()` throws, show the raw source in a `<pre>` fallback rather than crashing.

`mermaid` is a large package (~21 deps). Import it with a dynamic `import('mermaid')` inside the `useEffect` to keep it out of the initial bundle.

---

### Task 4 — Path deeplinks in chat messages

**Modified file:** `web/src/components/MessageBubble.tsx`

Before passing content to ReactMarkdown, run a pre-processing pass that converts bare file paths into markdown links. The link target uses a custom `file://` scheme that the app intercepts client-side.

**Detection regex:**
```
/(\/(?:Users\/[^/\s]+|Volumes\/[^/\s]+)[/\S]*)/g
```

This matches:
- `/Users/<anything>/...` — paths under home
- `/Volumes/<anything>/...` — mounted drives

Paths appearing inside existing markdown code fences or inline code spans must be excluded (they should render as code, not links). A simple approach: only run the replacement on lines that do not start with ` ``` ` or contain surrounding backticks.

The generated markdown link looks like: `[/path/to/file](/path/to/file)`.

In the ReactMarkdown `components` map, add an `a` override that checks whether the `href` starts with `/Users/` or `/Volumes/`. If so, render a styled `<PathChip>` component instead of a normal anchor:

- `PathChip` renders the path with a small folder or file icon prefix (determined by extension presence — no extension = directory, extension = file)
- On click: prevent default, call a prop/callback `onFilePath(href)` which opens `FileExplorerModal` at that path
- Styled with `font-family: var(--font-mono)`, small font size, subtle background, rounded pill shape — visually distinct from regular links but not jarring

The `onFilePath` callback is threaded down from `MessageBubble`'s caller. In `ChatView` and `MobileChatView`, this opens the `FileExplorerModal` with the clicked path pre-selected.

---

### Task 5 — FileExplorerModal component

**New files:** `web/src/components/FileExplorerModal.tsx`, `web/src/components/FileExplorerModal.module.css`

The modal is a full-screen overlay (on mobile) or a large centered dialog (on desktop, max ~85vw × 80vh). It has two panels:

**Left panel — directory tree**

Uses `react-arborist`. The tree data is fetched lazily: the root node is the starting path, and children are fetched from `fsApi.list()` when a directory node is expanded. This avoids loading the entire subtree upfront.

Node renderer (the child render function `react-arborist` requires):
- Indent level is provided by arborist
- Folder icon (▶ collapsed, ▼ expanded) for directories; file icon for files
- File extension badge for known types (`.md`, `.rs`, `.ts`, `.json`, `.png`, etc.)
- Selected node gets `background: var(--accent-primary)` with white text
- Hover state: `background: var(--bg-elevated)`

`react-arborist` requires explicit `height` (px) for virtualization. Wrap the tree panel in a `ResizeObserver` that passes the measured pixel height to the `<Tree>` component.

Breadcrumb bar above the tree shows the current root path, split at `/`, with each segment clickable to navigate up.

**Right panel — file preview**

When a file node is selected in the tree, fetch `fsApi.read(path)` and render:

- **Markdown (`.md`):** ReactMarkdown with the same plugins as the chat renderer, including mermaid support — file previews benefit from this immediately
- **Code files (`.ts`, `.tsx`, `.rs`, `.py`, `.js`, `.json`, `.toml`, `.yaml`, etc.):** `<pre>` with `font-family: var(--font-mono)` and the file's extension as a language label
- **Images:** `<img>` with `max-width: 100%`
- **Binary / too large:** A centred message with the file name, size, and reason

Loading state: spinner while `fsApi.read` is in flight. Error state: error message with a retry button.

**Opening the modal**

The modal accepts these props:
```ts
interface FileExplorerModalProps {
  isOpen: boolean;
  initialPath: string;   // directory or file path to navigate to on open
  onClose: () => void;
}
```

When `initialPath` is a file, the tree opens its parent directory and auto-selects the file, loading its preview immediately.

**State managed inside the modal:**
- `rootPath: string` — the directory currently shown as the tree root (starts as home dir or workspace dir)
- `selectedPath: string | null` — the currently selected file
- `treeData: TreeNode[]` — arborist node array, built lazily

**Close button** and `Escape` key close the modal. On mobile the modal fills the entire screen with a back button replacing the close button; the tree and preview panels stack vertically (tree on top, preview below), switching between them when a file is selected.

---

### Task 6 — Explorer trigger in thread UI

**Modified files:** `web/src/components/ChatView.tsx`, `web/src/layouts/mobile/MobileChatView.tsx`

Add state to each: `explorerOpen: boolean` and `explorerInitialPath: string`.

**Folder button in the chat header:**  
Add a small folder icon button to `ChatHeader.tsx` (desktop) and the equivalent header area in `MobileChatView.tsx`. On click: call `fsApi.workspace(thread.id)`, then open `FileExplorerModal` with the returned path as `initialPath`. This is the primary way a user browses the thread's workspace.

**Path deeplink callback:**  
Wire `onFilePath` from Task 4 through `MessageBubble` → `ChatView`/`MobileChatView` → `FileExplorerModal`. When the user clicks a path chip in chat, `explorerInitialPath` is set to that path and `explorerOpen` becomes true.

`FileExplorerModal` is rendered at the bottom of the `ChatView` and `MobileChatView` return trees, conditionally on `explorerOpen`.

---

### Task 7 — Workspace system prompt injection

**Modified file:** `server/src/services/context.rs`

The `AssemblyInput` struct already carries per-run context that is injected into the system prompt. Add a `workspace_path: Option<String>` field.

In `run_inner` (agent.rs), before calling `generation_loop`, resolve the thread's workspace path using the same logic as the `get_workspace` route (create if absent, return the path). Pass it in `AssemblyInput`.

In `assemble` (context.rs), if `workspace_path` is `Some`, append a short block to the system prompt:

```
---
Workspace directory for this thread: /Users/marcus/agent-deck-workspaces/{thread-id}
You may read and write files here using your available tools. Reference files using their full absolute path so the user can navigate to them.
```

This gives the agent a consistent, thread-scoped scratch space without requiring any new tools.

---

### Parallelisation note

Tasks 1 and 2 (server routes + API client types) can be built in parallel with Tasks 3 and 4 (mermaid + path deeplinks), since they touch entirely separate files. Task 5 (modal) depends on Task 2 (types + fsApi). Task 6 (trigger wiring) depends on Task 5. Task 7 (workspace injection) depends on Task 1 (the workspace route logic can be extracted into a shared service function called by both the route and agent.rs).

Suggested parallel split:
- **Agent A:** Tasks 1 + 7 (all Rust)
- **Agent B:** Tasks 2 + 3 + 4 (types, mermaid, deeplinks — web only, disjoint files except MessageBubble.tsx which Agent B owns entirely)
- **Sequential:** Task 5, then Task 6 (depend on both agents' output)

---

## Acceptance Criteria

- [ ] `GET /api/fs/list?path=~` returns the home directory's entries sorted dirs-first alphabetically
- [ ] `GET /api/fs/list?path=/Volumes` returns mounted drive entries
- [ ] `GET /api/fs/list?path=/etc` returns 403 Forbidden
- [ ] `GET /api/fs/list?path=/Users/x/../etc` returns 403 Forbidden (canonicalize defeats traversal)
- [ ] `GET /api/fs/read` on a `.md` file returns `previewable: true` with UTF-8 content
- [ ] `GET /api/fs/read` on a `.png` image returns `previewable: true`, `is_image: true`, and base64 data
- [ ] `GET /api/fs/read` on a binary file returns `previewable: false`, `reason: "binary"`
- [ ] `GET /api/fs/read` on a file over 5 MB returns `previewable: false`, `reason: "too_large"`
- [ ] `GET /api/fs/workspace?thread_id=abc` creates `~/agent-deck-workspaces/abc/` if it does not exist and returns the path
- [ ] Mermaid fenced code blocks in agent messages render as SVG diagrams, not raw text
- [ ] Mermaid render errors fall back to displaying the raw source in a code block without crashing
- [ ] File paths matching `/Users/*/...` or `/Volumes/*/...` in agent messages render as styled clickable path chips
- [ ] Clicking a path chip opens `FileExplorerModal` pre-navigated to that path
- [ ] Other markdown links (http/https) are unaffected by the path detection
- [ ] Folder button in the desktop chat header opens `FileExplorerModal` at the thread's workspace directory
- [ ] Folder button in the mobile chat header opens `FileExplorerModal` at the thread's workspace directory
- [ ] `FileExplorerModal` shows the directory tree on the left and file preview on the right (desktop) or stacked (mobile)
- [ ] Expanding a directory node in the tree fetches its children lazily via `GET /api/fs/list`
- [ ] Selecting a file in the tree loads its preview via `GET /api/fs/read`
- [ ] Markdown files preview as rendered markdown (including mermaid diagrams)
- [ ] Image files preview as `<img>` elements
- [ ] Binary files show a "cannot preview" message with size
- [ ] The breadcrumb bar reflects the current root directory; tapping a segment navigates up
- [ ] On mobile the explorer is full-screen; selecting a file switches to the preview panel; a back button returns to the tree
- [ ] Pressing Escape or clicking the close button closes the modal
- [ ] The agent's system prompt includes the workspace path for every non-routine run
- [ ] The injected workspace path is correct for the thread being run

---

## Human Review Instructions

**Prerequisites:** Server running on port 7474. Have at least one thread open.

1. **Security boundary** — In the terminal, run:
   ```
   curl "http://localhost:7474/api/fs/list?path=/etc" -H "Cookie: <auth>"
   ```
   → **Expected:** 403 response. → **Failure:** Directory listing returned.

2. **Home directory listing** — Open the file explorer (folder button in thread header).  
   → **Expected:** Modal opens showing your home directory tree. Subdirectories expand on click. Files show name, extension, and size.

3. **Mounted drives** — If you have an external drive or network share mounted, expand `/Volumes` in the tree.  
   → **Expected:** Drive appears as a top-level entry and is navigable.

4. **Markdown preview** — Navigate to any `.md` file in the tree and select it.  
   → **Expected:** Right panel renders the markdown (headers, bold, lists, tables) — not raw text.

5. **Mermaid in chat** — Ask the agent to draw a simple flowchart using a mermaid code block.  
   → **Expected:** The chat bubble renders an SVG diagram, not a raw code block.

6. **Path deeplink** — Ask the agent to mention a file path (e.g. "check `~/Desktop/notes.txt`").  
   → **Expected:** The path renders as a styled chip. Clicking it opens the explorer at that path with the file selected and previewed.

7. **Workspace creation** — Open a thread that has never had a workspace.  
   Click the folder button.  
   → **Expected:** Modal opens at `~/agent-deck-workspaces/{thread-id}/`. The directory was just created. Check with `ls ~/agent-deck-workspaces/` in the terminal to confirm.

8. **Workspace in system prompt** — Send a message in a thread, then check the server logs.  
   → **Expected:** Log line shows workspace path was injected. The agent's response may reference the workspace directory if asked.

9. **Mobile explorer** — On a narrow viewport (or real device), tap the folder button.  
   → **Expected:** Full-screen explorer opens. Tree fills the screen. Selecting a file slides to the preview panel. Back button returns to the tree.

10. **Large / binary file** — Navigate to a binary file or a file over 5 MB.  
    → **Expected:** Preview panel shows "Cannot preview this file" with the file size and reason.

---

## Approval

- [ ] **Implementation plan approved** — human has reviewed this plan and confirmed coding can begin
- [ ] **Coding complete** — all tests pass, agent has verified against every acceptance criterion
- [ ] **Human review approved** — human has tested the changes live and signed off