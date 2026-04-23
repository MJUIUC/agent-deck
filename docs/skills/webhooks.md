# Skill: Register a Webhook Binding

You can set up webhook integrations on behalf of the user by calling the agent-deck REST API and, when credentials are available, registering the webhook on the external service as well. A webhook binding connects an external service's event stream to a thread — when the service sends a matching event, the platform verifies the signature and runs the agent with a natural-language summary of what happened.

---

## When to use this skill

Use this skill when the user asks you to:
- Connect a GitHub repo so the agent reacts to PRs, pushes, issues, or comments
- Connect a GitLab project so the agent reacts to merge requests, pushes, or issues
- Set up any external service to trigger this thread's agent via webhook
- List or remove existing webhook bindings on this thread

---

## How it works

All inbound webhook deliveries arrive at a single public URL (`/api/webhooks`). The **secret** is what identifies which binding the request belongs to — when the external service sends a delivery, the platform verifies the HMAC-SHA256 signature (or token, for GitLab's older format) using each stored binding's decrypted secret. The first match wins; the event is accepted and the agent runs asynchronously with a natural-language summary. No match → 401.

Webhook bindings are managed in two layers:

1. **Global registry** — a named binding (`POST /api/webhook-bindings`) that holds the source, event type, secret, and enabled state. It can be reused across threads.
2. **Thread attachment** — attaching a global binding to a specific thread (`POST /api/threads/{thread_id}/webhook-bindings`) so that thread's agent receives matching events. Each attachment can carry its own per-thread `prompt` (response instructions).

---

## Prerequisite: Tailscale Funnel

External services need a reachable public HTTPS URL. Check Funnel status before creating any binding:

**`GET /api/tailscale/status`**

```json
{
  "data": {
    "connected": true,
    "funnel_enabled": true,
    "funnel_url": "https://your-machine.tail1234.ts.net"
  }
}
```

If `funnel_enabled` is `false`, tell the user Funnel must be enabled first. Enable it via:

**`POST /api/tailscale/funnel/enable`**

Do not create a binding or configure the external service until `funnel_enabled` is `true`.

---

## Step 1 — Create the global binding

Register a named binding in the global registry. This is the same for all sources.

**`POST /api/webhook-bindings`**

```json
{
  "name": "my-repo — all events",
  "source": "github",
  "event_type": "*"
}
```

| Field | Value |
|---|---|
| `name` | A human-readable label (e.g. `"my-repo PRs"`) |
| `source` | `"github"`, `"gitlab"`, or `"generic"` |
| `event_type` | `"*"` to receive all events (recommended), or a specific event name |

**Response (secret shown only once):**

```json
{
  "data": {
    "id": "...",
    "name": "my-repo — all events",
    "source": "github",
    "event_type": "*",
    "webhook_url": "https://your-machine.tail1234.ts.net/api/webhooks",
    "secret": "abc123def456...",
    "enabled": true,
    "created_at": "..."
  }
}
```

If `funnel_enabled` was `false`, the response includes a `"warning"` field — surface it to the user.

**The `secret` is only returned on creation.** Present both `webhook_url` and `secret` to the user immediately and clearly. They cannot be recovered after this step.

---

## Step 1b — Attach the binding to this thread

After creating (or finding an existing) global binding, attach it to the current thread so this thread's agent receives matching events.

**`POST /api/threads/{thread_id}/webhook-bindings`**

```json
{
  "webhook_binding_id": "{id from step 1}",
  "prompt": "When a PR is opened, summarize the changes and check for obvious issues."
}
```

| Field | Value |
|---|---|
| `webhook_binding_id` | The `id` returned in Step 1 |
| `prompt` | *(optional)* Per-thread response instructions. Can be set here or updated later. |

**Response:**

```json
{
  "data": {
    "id": "...",
    "webhook_binding_id": "...",
    "name": "my-repo — all events",
    "source": "github",
    "event_type": "*",
    "enabled": true,
    "prompt": "When a PR is opened, summarize the changes and check for obvious issues.",
    "created_at": "..."
  }
}
```

The `id` in this response is the **attachment id** — use it for detaching or updating the prompt on this thread.

The same global binding can be attached to multiple threads. Each attachment carries its own independent `prompt`.

---

## Setting response instructions

Each thread attachment can have a `prompt` field — instructions for how the agent should respond when that specific event fires. When set, the agent receives both the event summary and the instructions as a combined message.

Include it at attachment time (see Step 1b above), or update it later:

**`PATCH /api/threads/{thread_id}/webhook-bindings/{attachment_id}`**
```json
{
  "prompt": "Summarize what changed and flag any files that touch the payments module."
}
```

Set `"prompt": null` to clear the instructions and return to default behavior (the agent receives only the event summary).

Without a prompt, the agent decides how to respond based on the event summary alone. With a prompt, the combined message is:
```
{event summary}

{binding prompt}
```

---

## Step 2 — Register on the external service

After creating the binding, register the webhook on the external service. Automate this when you have credentials; otherwise give the user the manual steps.

---

### GitHub

**Check for a stored GitHub credential first:**

**`GET /api/credentials`**

Look for a credential with `service: "github"` or `credential_type: "pat"`. If one exists, you can automate the GitHub registration. If not, ask the user if they have a GitHub Personal Access Token — if yes, store it first:

**`POST /api/credentials`**
```json
{
  "key": "github_pat",
  "display_name": "GitHub Personal Access Token",
  "service": "github",
  "credential_type": "pat",
  "secret": "<the PAT>"
}
```
The PAT needs the `admin:repo_hook` scope.

**Register the webhook via the GitHub API:**

```
POST https://api.github.com/repos/{owner}/{repo}/hooks
Authorization: Bearer {decrypted_github_pat}
Content-Type: application/json

{
  "name": "web",
  "active": true,
  "events": ["*"],
  "config": {
    "url": "{webhook_url}",
    "content_type": "json",
    "secret": "{secret}",
    "insecure_ssl": "0"
  }
}
```

Map `event_type` to the GitHub events array:

| Binding event_type | GitHub events value |
|---|---|
| `"*"` | `["*"]` (all events) |
| `"pull_request"` | `["pull_request"]` |
| `"push"` | `["push"]` |
| `"issues"` | `["issues"]` |
| `"issue_comment"` | `["issue_comment"]` |

You can make this API call using a terminal MCP (`curl`) or the GitHub MCP server if it's attached to this thread.

**If you cannot automate** (no PAT, no terminal, no GitHub MCP), give the user these steps:
1. GitHub repo → **Settings → Webhooks → Add webhook**
2. **Payload URL**: paste `webhook_url`
3. **Content type**: `application/json`
4. **Secret**: paste `secret`
5. **Which events**: "Send me everything" for `"*"`, or select specific events
6. Click **Add webhook**

GitHub sends a `ping` event on save — this does not trigger the agent, but a green checkmark confirms the URL and secret are correct.

**Events the agent understands:**

| GitHub event | Agent prompt |
|---|---|
| `pull_request` (opened) | `GitHub: New PR #N opened by @user — "title". Base: branch. url` |
| `pull_request` (review_requested) | `GitHub: Review requested on PR #N — "title" from @reviewer. url` |
| `issue_comment` | `GitHub: @user commented on issue/PR #N — "body…". url` |
| `push` | `GitHub: N commit(s) pushed to ref by @user. Latest: "message". url` |
| `issues` (opened) | `GitHub: Issue #N opened by @user — "title". url` |
| Other | `GitHub webhook: event=X, action=Y. Repo: org/repo.` |

---

### GitLab

**Check for a stored GitLab credential:**

**`GET /api/credentials`**

Look for `service: "gitlab"`. If none, ask the user for a GitLab Personal Access Token with `api` scope. Store it:

**`POST /api/credentials`**
```json
{
  "key": "gitlab_pat",
  "display_name": "GitLab Personal Access Token",
  "service": "gitlab",
  "credential_type": "pat",
  "secret": "<the PAT>"
}
```

**Register the webhook via the GitLab API:**

```
POST https://gitlab.com/api/v4/projects/{project_id}/hooks
PRIVATE-TOKEN: {decrypted_gitlab_pat}
Content-Type: application/json

{
  "url": "{webhook_url}",
  "token": "{secret}",
  "push_events": true,
  "merge_requests_events": true,
  "issues_events": true,
  "note_events": true,
  "enable_ssl_verification": true
}
```

For self-hosted GitLab, replace `gitlab.com` with the instance hostname. The `project_id` can be a numeric ID or `owner%2Frepo` (URL-encoded).

You can set individual event toggles (`push_events`, `merge_requests_events`, etc.) to `false` if the binding's `event_type` is not `"*"`.

**If you cannot automate**, give the user these manual steps:
1. GitLab project → **Settings → Webhooks → Add new webhook**
2. **URL**: paste `webhook_url`
3. **Secret token**: paste `secret`
4. Check the events you want to receive
5. Click **Add webhook**

**Events the agent understands:**

| GitLab event | Agent prompt |
|---|---|
| `Push Hook` | `GitLab: N commit(s) pushed to ref by user. Latest: "message". url` |
| `Merge Request Hook` (open) | `GitLab: New MR !N opened by user — "title". Target: branch. url` |
| `Merge Request Hook` (merge) | `GitLab: MR !N merged — "title". url` |
| `Issue Hook` (open) | `GitLab: Issue #N opened by user — "title". url` |
| `Note Hook` | `GitLab: user commented — "note…". url` |
| Other | `GitLab webhook: event=X. Project: org/repo.` |

---

### Generic (any other service)

For services like Stripe, Linear, Shopify, or any other webhook source:

1. Create the binding with `"source": "generic"` and `"event_type": "*"`
2. Give the user the `webhook_url` and `secret`
3. In the service's webhook settings, configure it to sign requests using HMAC-SHA256 and send the signature as `X-Webhook-Signature: <hex>` (this is the header the platform checks for generic bindings)

Services vary in how they sign webhooks. If the service uses a different signature header or format, tell the user:
- The platform receives the request at `webhook_url`
- The HMAC signature must be in `X-Webhook-Signature` as a plain hex string (no prefix)
- If the service uses a different header name or format, this won't match automatically — check the service's documentation to confirm compatibility

The generic formatter produces:
```
Webhook received from {source}: {pretty-printed payload, truncated to 500 chars}
```

---

## Managing bindings

Webhook management operates at two levels.

### Global registry

**List all global bindings** (check before creating duplicates):

**`GET /api/webhook-bindings`**

The `secret` is never included in list or get responses.

**Delete a global binding:**

**`DELETE /api/webhook-bindings/{id}`**

This removes the binding from the global registry and detaches it from **all threads** that had it attached. After deletion, also remove the corresponding webhook from the external service's settings to stop it sending events.

**Toggle a global binding on/off:**

**`PATCH /api/webhook-bindings/{id}/toggle`**

Disabling a global binding suppresses event processing for all threads it is attached to without removing the attachment.

### Thread attachments

**List this thread's attached bindings:**

**`GET /api/threads/{thread_id}/webhook-bindings`**

**Detach a binding from this thread only:**

**`DELETE /api/threads/{thread_id}/webhook-bindings/{attachment_id}`**

This removes the attachment from this thread only — the global binding and any other thread attachments are unaffected.

**Update per-thread response instructions:**

**`PATCH /api/threads/{thread_id}/webhook-bindings/{attachment_id}`**

```json
{ "prompt": "New instructions…" }
```

Pass `"prompt": null` to clear the instructions.

---

## Error cases

| Error | Cause | Resolution |
|---|---|---|
| `"warning"` in creation response | Funnel is off | Enable via `POST /api/tailscale/funnel/enable` |
| 401 in external service delivery log | HMAC mismatch — secret copied wrong | Delete binding, create new one, copy secret carefully (no extra whitespace) |
| 202 returned but agent never runs | `event_type` mismatch | Delete binding, create new one with `"*"` |
| Red X / failed delivery in service dashboard | Wrong URL or server unreachable | Confirm Funnel is enabled and `funnel_url` matches the configured URL |
| GitHub API `404` when registering hook | Wrong owner/repo or insufficient PAT scope | Confirm repo path is correct; PAT needs `admin:repo_hook` scope |
| GitLab API `403` when registering hook | Insufficient PAT scope | PAT needs `api` scope |