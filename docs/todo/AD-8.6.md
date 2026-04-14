# AD-8.6 — Multimedia / File Upload

**Story:** 8.6 — Multimedia / File Upload  
**Branch:** `feature/phase8-file-upload`  
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 8  
**Depends on:** Story 8.3a complete (file explorer + workspace directories)

---

## Summary

Users and agents can currently only exchange plain text. This story adds multipart file upload to the message input — users drag-and-drop or pick an image from their device, the file is stored in the thread's workspace directory, and the message is sent with an image attachment that vision-capable providers can see. A provider capability flag (`vision: bool`) gates the upload affordance so it only appears when the active model supports image input. Non-image files (PDF, text, code) are uploaded to the workspace and shared as a download link — the agent sees the file path and can read it via tools.

---

## Current State

### What already exists

- `server/src/routes/mod.rs` — authenticated router
- `server/src/routes/messages.rs` — `POST /api/threads/:id/messages` handler; `CreateMessage` accepts `role` and `content` (string)
- `server/src/services/agent.rs` — `run_inner` assembles context and calls the provider
- `server/src/services/context.rs` — `AssemblyInput`, `assemble()` — builds the message array sent to the LLM
- `server/src/routes/fs.rs` — `validate_path`, `get_workspace` — workspace path resolution and security boundary
- `web/src/components/ChatView.tsx` — desktop message input with send button
- `web/src/layouts/mobile/MobileChatView.tsx` — mobile message input
- `web/src/api/client.ts` — `messagesApi.send(threadId, content)` 
- `web/src/types/index.ts` — `Message`, `Thread`, `Provider` types
- Provider records in DB: `providers` table has `base_url`, `api_key`, `kind`
- `async-openai` crate: already used for chat completions; supports `ChatCompletionRequestMessageContentPart` (text + image_url)

### What does not exist yet

- `POST /api/threads/:id/upload` — multipart upload endpoint
- `providers.vision` column / vision capability flag
- `CreateMessage.attachments` field
- Image content part support in `context.rs` `assemble()`
- Upload UI in `ChatView.tsx` and `MobileChatView.tsx`
- Provider settings: vision toggle
- `uploadsApi` in `client.ts`

---

## Implementation Plan

### Task 1 — Schema: `vision` flag on providers

**New migration:** `server/src/db/migrations/014_provider_vision.sql`

```sql
ALTER TABLE providers ADD COLUMN vision INTEGER NOT NULL DEFAULT 0;
```

**Modified files:** `server/src/models/provider.rs`, relevant SELECT queries in `routes/providers.rs`

Add `vision: bool` to the `Provider` struct. Update all `SELECT` queries that return `Provider` rows to include the `vision` column. Add `vision` to `CreateProvider` and `UpdateProvider` request structs so it can be toggled via `PUT /api/providers/:id`.

Update the provider settings form (desktop: `web/src/components/settings/ProviderSettings.tsx`, mobile: the provider form drawer in `MobileSettings.tsx`) to include a "Vision / image input" toggle below the base URL field.

### Task 2 — Server: `POST /api/threads/:id/upload`

**New route handler in:** `server/src/routes/messages.rs` (or a new `server/src/routes/uploads.rs`)  
**Modified file:** `server/src/routes/mod.rs`

```
POST /api/threads/:id/upload
Content-Type: multipart/form-data
```

Handler steps:
1. Verify thread ownership (same guard as `post_message`)
2. Parse multipart body using `axum-multipart` — extract `file` field
3. Read filename and content type from the part headers
4. Resolve the thread's workspace path via the same logic as `get_workspace` (create if absent)
5. Sanitize the filename: strip path separators, limit to 200 chars, preserve extension
6. If a file with the same name exists, append a numeric suffix (`report.pdf` → `report_1.pdf`)
7. Write file bytes to `<workspace_path>/<sanitized_filename>` using `tokio::fs::write`
8. Return:
```json
{
  "data": {
    "path": "/Users/marcus/agent-deck-workspaces/<thread-id>/image.png",
    "filename": "image.png",
    "size": 204800,
    "content_type": "image/png",
    "is_image": true
  }
}
```

`is_image` is `true` when `content_type` starts with `image/`.

File size limit: **20 MB** — return `413 Payload Too Large` if exceeded. Read `content_length` from part headers before buffering to fail fast.

No new Cargo dependency needed — `axum` already ships multipart support via `axum::extract::Multipart`.

### Task 3 — Context assembly: image attachments

**Modified files:** `server/src/services/context.rs`, `server/src/services/agent.rs`

`CreateMessage` gains an optional `attachments` field:

```rust
pub struct CreateMessage {
    pub role: String,
    pub content: String,
    pub attachments: Option<Vec<MessageAttachment>>,
}

pub struct MessageAttachment {
    pub path: String,         // absolute path on disk
    pub content_type: String, // "image/png", "application/pdf", etc.
    pub filename: String,
}
```

`AssemblyInput` gains `attachments: Option<Vec<MessageAttachment>>` on the triggering user message.

In `assemble()`, when the user message has image attachments and the provider's vision flag is `true`, replace the plain-text `ChatCompletionRequestUserMessage` content with a `Vec<ChatCompletionRequestMessageContentPart>`:

```rust
// text part
ChatCompletionRequestMessageContentPart::Text(ChatCompletionRequestMessageContentPartText {
    text: content.clone(),
})
// image part (one per image attachment)
ChatCompletionRequestMessageContentPart::ImageUrl(ChatCompletionRequestMessageContentPartImage {
    image_url: ImageUrl {
        url: format!("data:{};base64,{}", attachment.content_type, base64_encoded_bytes),
        detail: Some(ImageDetail::Auto),
    },
})
```

Images are base64-encoded inline (`data:` URI) — no external hosting needed. File bytes are read from disk at context-assembly time using `tokio::fs::read`.

For non-image attachments (PDF, text, code, etc.) or when vision is `false`: the attachment is not sent to the model as an image part. Instead, a note is appended to the user message content:

```
[Attached file: report.pdf — available at /Users/marcus/agent-deck-workspaces/<id>/report.pdf]
```

This lets the agent acknowledge the file and use its file-reading tools to access it.

`messages` table: the existing `content TEXT` column stores the user's text. Attachments are stored as a JSON string in a new `attachments TEXT` column (added in the migration below). This allows message history reconstruction for multi-turn conversations with images.

**Additional migration change** (add to migration 014):
```sql
ALTER TABLE messages ADD COLUMN attachments TEXT;  -- JSON array of MessageAttachment
```

Update `Message` model struct and `SELECT` queries accordingly.

### Task 4 — Upload UI (desktop and mobile)

**Modified files:**
- `web/src/components/ChatView.tsx`
- `web/src/components/ChatView.module.css`
- `web/src/layouts/mobile/MobileChatView.tsx`

**Upload affordance:**

A paperclip icon (📎) button sits to the left of the send button in the message input bar. It is only rendered when `thread.provider.vision === true` OR always shown but grayed out when vision is false (TBD by UX preference — simpler to always show and explain on hover).

Clicking the button opens a hidden `<input type="file" accept="image/*,application/pdf,text/*">` element.

On file selection (or drag-and-drop onto the input area):
1. Show a thumbnail chip above the input bar:
   - Image files: small `<img>` thumbnail (URL.createObjectURL)
   - Non-image files: file icon + filename + size
   - Each chip has an `×` button to remove the attachment before sending
2. The chip is purely local state — no upload yet

On send:
1. If attachments exist, POST each to `/api/threads/:id/upload` (sequential, one at a time)
2. Collect the returned `path` and `content_type` values
3. POST the message to `/api/threads/:id/messages` with `attachments` array in the body
4. Clear the attachment chips

Loading state during upload: show a spinner on the chip, disable the send button.

**`uploadsApi` in `client.ts`:**
```ts
export const uploadsApi = {
  upload(threadId: string, file: File): Promise<{ data: UploadedFile }> {
    const form = new FormData();
    form.append('file', file);
    return apiFetch(`/api/threads/${threadId}/upload`, { method: 'POST', body: form });
  },
};
```

Add `UploadedFile` and `MessageAttachment` to `web/src/types/index.ts`.

**Message bubble rendering:**

In `web/src/components/MessageBubble.tsx`, when `message.attachments` is non-null and non-empty, render attachments above the message text:
- Images: `<img src="data:..." />` reconstructed from the stored attachment path (fetched via `fsApi.read` on render) — or better, store a thumbnail in the message record
- Non-images: a styled file chip with download link (`/api/fs/download?path=...`)

To avoid re-reading files on every render, attachments are rendered using the `/api/fs/read?path=` URL for images (the browser caches the response) and `/api/fs/download?path=` as the href for non-images.

### Parallelisation note

Task 1 (schema) must be done first — Tasks 2 and 3 depend on it. Task 2 (upload endpoint) and Task 3 (context assembly) can be built in parallel after Task 1. Task 4 (UI) depends on Task 2 (the upload endpoint URL) but not on Task 3 internals.

Suggested split:
- **Server agent:** Tasks 1 + 2 + 3 (all Rust — migration, upload route, context changes)
- **Frontend agent:** Task 4 (React UI + `uploadsApi`)
- **Sequential:** message bubble rendering (depends on Task 3's `attachments` field being in the message response)

---

## Acceptance Criteria

- [ ] Migration 014 adds `vision INTEGER NOT NULL DEFAULT 0` to `providers` and `attachments TEXT` to `messages`
- [ ] `PUT /api/providers/:id` accepts and persists `vision` field
- [ ] Vision toggle appears in desktop provider settings form
- [ ] Vision toggle appears in mobile provider add/edit form
- [ ] `POST /api/threads/:id/upload` accepts a multipart file and writes it to the thread workspace
- [ ] Upload returns the absolute path, filename, size, content_type, and is_image flag
- [ ] Files larger than 20 MB return `413 Payload Too Large`
- [ ] Upload endpoint enforces the same path security boundary as `GET /api/fs/read`
- [ ] Filename is sanitized (no path separators; numeric suffix on collision)
- [ ] Paperclip button appears in the desktop message input bar
- [ ] Paperclip button appears in the mobile message input bar
- [ ] Clicking the button opens a file picker
- [ ] Dragging a file onto the input area also triggers selection
- [ ] Selected image shows a thumbnail chip above the input bar
- [ ] Selected non-image shows a file icon chip with filename
- [ ] `×` button on a chip removes the attachment before sending
- [ ] On send, each attachment is uploaded before the message is posted
- [ ] Send button is disabled during upload
- [ ] Spinner appears on the chip during upload
- [ ] Sent message with image attachment: the image is visible in the chat bubble
- [ ] Sent message with non-image attachment: a download chip appears in the chat bubble
- [ ] When vision is true, image attachments are sent to the model as `image_url` content parts
- [ ] When vision is false, image attachments are sent as a text note with the file path
- [ ] Non-image attachments are always sent as a text note with the file path
- [ ] `cargo build` passes, all existing tests pass

---

## Human Review Instructions

*To be filled in after coding is complete (Step 5 of AGENT_WORKFLOW).*

---

## Approval

- [ ] **Implementation plan approved**
- [ ] **Coding complete**
- [ ] **Human review approved**
