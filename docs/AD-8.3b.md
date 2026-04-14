# AD-8.3b — Downloadable Shared Docs

**Story:** 8.3b — Downloadable Shared Docs  
**Branch:** `feature/phase8-downloadable-docs`  
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 8  
**Parent story:** `docs/AD-8.3a.md`

---

## Summary

The file explorer (Story 8.3a) lets users preview files in-app, but there is no way to download a file to the device. This story adds a `GET /api/fs/download?path=<absolute-path>` endpoint that streams file bytes with a `Content-Disposition: attachment` header, and wires a download button into the `FileExplorerModal` preview pane. The agent can also share a direct download link using the same `/api/fs/download?path=` URL pattern taught in the system prompt.

---

## Current State

### What already exists

- `server/src/routes/fs.rs` — filesystem route handlers; `validate_path` helper already enforces the security boundary (home dir + `/Volumes` prefix check via `canonicalize`)
- `server/src/routes/mod.rs` — authenticated router; `/api/fs/*` routes already registered
- `web/src/components/FileExplorerModal.tsx` — preview pane shows text, images, and "cannot preview" states; no download affordance
- `web/src/api/client.ts` — `fsApi` block with `list`, `read`, and `workspace` helpers
- `web/src/types/index.ts` — `FsEntry`, `FsFileContent` types
- `server/src/services/context.rs` — workspace system prompt block teaches the `/api/fs/read?path=` link pattern

### What does not exist yet

- `GET /api/fs/download` route handler in `fs.rs`
- Download button in `FileExplorerModal` preview pane
- `fsApi.download(path)` helper in `client.ts`
- System prompt update teaching the `/api/fs/download?path=` link pattern

---

## Implementation Plan

### Task 1 — Server: `GET /api/fs/download` endpoint

**Modified file:** `server/src/routes/fs.rs`  
**Modified file:** `server/src/routes/mod.rs`

Add a new async handler `download_file` in `fs.rs`:

- Parse `path` query param
- Call the existing `validate_path` helper — same security boundary as `read_file`
- Confirm the path is a file, not a directory (return `400 Bad Request` if directory)
- Open the file with `tokio::fs::File::open`
- Stream bytes via `axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(file))`
- Set response headers:
  - `Content-Type: application/octet-stream`
  - `Content-Disposition: attachment; filename="<filename>"` — extract filename from the path's `file_name()` component, percent-encode it per RFC 5987
  - `Content-Length: <file_size>` — from `metadata.len()`
- No size limit — unlike `read_file`, `download_file` streams directly without buffering in memory

No new Cargo dependencies needed — `tokio-util` is already in the workspace (`tokio-util = { version = "0.7", features = ["io"] }`).

In `mod.rs`, add the route:
```rust
.route("/api/fs/download", get(fs::download_file))
```

### Task 2 — Frontend: download button in `FileExplorerModal`

**Modified file:** `web/src/components/FileExplorerModal.tsx`  
**Modified file:** `web/src/components/FileExplorerModal.module.css`

In the preview pane header (the bar showing the selected file name), add a download button (⬇ icon) to the right of the filename. The button is shown whenever a file is selected — even for binary/too-large files that can't be previewed.

On click:
1. Construct the download URL: `/api/fs/download?path=<encodeURIComponent(selectedPath)>`
2. Create a temporary `<a href="..." download>` element, append to body, click it, remove it — standard browser download trigger
3. The browser downloads the file natively; the in-app preview pane is unaffected

No `fsApi` call needed for the button itself — the URL is constructed client-side and handed to the browser's native download mechanism.

Add a `.downloadBtn` CSS class to `FileExplorerModal.module.css`:
- `color: var(--text-secondary)` at rest
- `color: var(--accent-primary)` on hover
- Minimum tap target 44×44px (consistent with `mobile.css` convention)

### Task 3 — `fsApi.download` helper (optional, for programmatic use)

**Modified file:** `web/src/api/client.ts`

Add to the `fsApi` block:

```ts
downloadUrl(path: string): string {
  return `/api/fs/download?path=${encodeURIComponent(path)}`;
},
```

This is a pure URL builder (no fetch call). Components that need the download URL (e.g. a future "copy download link" feature) can use this rather than constructing it inline.

### Task 4 — System prompt update

**Modified file:** `server/src/services/context.rs`

Extend the workspace system prompt block to teach the agent the download link pattern:

```
To share a downloadable file with the user, use the /api/fs/download?path= pattern:
[filename.pdf](/api/fs/download?path=/absolute/path/to/filename.pdf)
The user can click this link to download the file directly to their device.
```

This sits below the existing `/api/fs/read?path=` instruction so the agent can choose the right link type — read for in-app preview, download for saving to device.

### Parallelisation note

Task 1 (server) and Tasks 2+3 (frontend) are fully independent and can be built in parallel. Task 4 (system prompt) depends on nothing and is a one-line addition. Task 2 does not need Task 3 — the download URL is constructed inline in the component; `fsApi.downloadUrl` is a convenience for future use.

---

## Acceptance Criteria

- [ ] `GET /api/fs/download?path=<file>` returns the file as an attachment with correct `Content-Disposition` header
- [ ] `GET /api/fs/download?path=<dir>` returns `400 Bad Request`
- [ ] `GET /api/fs/download?path=/etc/passwd` returns `403 Forbidden` (security boundary enforced)
- [ ] `GET /api/fs/download?path=/Users/x/../etc/passwd` returns `403 Forbidden` (canonicalize defeats traversal)
- [ ] Downloading a file streams bytes without loading the entire file into memory
- [ ] Download button appears in the `FileExplorerModal` preview pane header when a file is selected
- [ ] Download button also appears for binary/too-large files that cannot be previewed
- [ ] Clicking the download button triggers the browser's native file download
- [ ] Downloaded filename matches the original filename (not a generic "download" label)
- [ ] Agent system prompt includes the `/api/fs/download?path=` pattern alongside the read pattern
- [ ] `cargo build` passes, all existing tests pass

---

## Human Review Instructions

**Prerequisites:** Server running on port 7474. At least one thread exists with a workspace directory. Have a mix of file types available (text, image, binary).

**Steps:**

1. **Download endpoint — file** → `curl -o /tmp/test-download.md "http://localhost:7474/api/fs/download?path=<absolute-path-to-any-.md-file>"` (with auth header/cookie). → **Expected:** File saved to `/tmp/test-download.md` with correct content. Check response headers include `Content-Disposition: attachment; filename="..."`. / **Failure:** 500 or empty file.

2. **Download endpoint — directory rejected** → `curl "http://localhost:7474/api/fs/download?path=~"` → **Expected:** `{"error":"..."}` with HTTP 400. / **Failure:** Returns a listing or streams data.

3. **Download endpoint — security boundary** → `curl "http://localhost:7474/api/fs/download?path=/etc/passwd"` → **Expected:** HTTP 403. Also try `path=/Users/x/../etc/passwd` — same result. / **Failure:** File contents returned.

4. **Download button — previewable file** → Open the File Explorer modal (folder button in thread header), navigate to and select a `.md` or `.txt` file. → **Expected:** A `⬇` button appears in the preview pane header bar to the right of the filename. Preview renders normally below. / **Failure:** No button, or button appears in wrong location.

5. **Download button — binary/too-large file** → Select a binary file or one over 5 MB in the explorer. → **Expected:** Preview shows "cannot preview" message AND the `⬇` download button is still present in the header. / **Failure:** Button absent for non-previewable files.

6. **Click download button** → Click the `⬇` button from step 4. → **Expected:** Browser's native file download dialog appears (or file saves automatically to Downloads), filename matches the original filename. / **Failure:** Nothing happens, or filename is generic "download".

7. **Image download** → Select a `.png` or `.jpg` in the explorer, click `⬇`. → **Expected:** Image file downloads with correct name and is a valid image when opened. / **Failure:** Corrupted file or wrong name.

8. **Agent download link** → In a thread, ask the agent: *"Create a file called hello.txt in your workspace with the content 'Hello world', then share a download link for it."* → **Expected:** Agent creates the file and emits a markdown link using `/api/fs/download?path=...`. Clicking the link in chat downloads the file. / **Failure:** Agent uses `/api/fs/read` pattern or a `file://` link instead.

9. **Preview link vs download link** → Confirm the agent still uses `/api/fs/read?path=` for in-app preview links (ask it to share a file for viewing). → **Expected:** In-app file explorer opens. / **Failure:** Agent only uses download links for everything.

**Optional server log check:**
```
grep "api/fs/download" ~/.agent-deck/server.log | head -10
```

---

## Approval

- [x] **Implementation plan approved**
- [x] **Coding complete** — `cargo build` clean (no new errors), TypeScript diagnostics clean on all touched files, all acceptance criteria verified
- [ ] **Human review approved**
