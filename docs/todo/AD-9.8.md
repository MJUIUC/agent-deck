# AD-9.8 — Chat UX Polish

**Story:** 9.8 — Chat UX Polish  
**Branch:** `feature/phase9-chat-ux-polish`  
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 9  
**Related stories:** `docs/AD-9.7.md` (tool progress indicators — also chat UX)

---

## Summary

A lightweight bucket story for small, high-value chat interface improvements that don't warrant their own full AD doc. Each item is self-contained and can be implemented independently. This story will accumulate items over time; items can be promoted to their own AD doc if scope grows.

---

## Items

### Item 1 — Message Referencing (Link-to-Message)

#### Problem

When a user wants to reference a specific past message in a conversation — to point the agent to it, ask a follow-up, or clarify context — they must manually copy and paste the message content. There is no way to "point" to a message directly.

#### Proposed UX

- Hovering over any message (user or assistant) reveals a small **link icon** (🔗 or `#`) in the message action bar
- Clicking it inserts a **clickable message reference link** into the composer input:

```
[↩ Marcus, 2:34 PM](#message-<turn_id>)

|  (cursor here, ready to type follow-up)
```

- Clicking the inserted link in a sent message **scrolls the thread to that original message** and briefly highlights it
- The reference includes: speaker label and timestamp
- The composer gains focus automatically after the reference is inserted

#### Key Distinction from Quoting

This is **not** a quote/copy of the message text. It is a **navigable reference** — a link that points back to the original message in the thread. The agent can see it as a structured reference, and the user can click it to jump directly to the referenced message. No text is duplicated into the new message.

#### Why this helps

- Lets the user point the agent to a specific message without copy-pasting content
- Keeps the new message clean — no large quoted blocks bloating the composer
- The agent receives a structured reference it can potentially use to look up the original turn
- Clicking the reference in the chat log jumps directly to the original message (smooth scroll + highlight)
- Unique to agent-deck — more powerful than Discord/Slack-style quoting

#### Scope

- **Primarily frontend** — message anchoring via `id` attributes on message elements
- Each message DOM element gets an `id="message-<turn_id>"` anchor
- The inserted link uses a standard in-page anchor `href="#message-<turn_id>"`
- On click: smooth scroll to the referenced message + brief highlight animation (e.g. yellow flash)
- Optional future backend enhancement: agent could resolve the `turn_id` to retrieve full message content if needed

#### Implementation notes

- Each message already has a `turn_id` — use this as the anchor ID
- On hover (desktop), show action bar with a link/reference button
- On reference click: format the markdown link string, append to current composer value, focus composer
- Mobile: long-press to reveal the action menu (hover doesn't exist on touch)
- Highlight animation: CSS class added on scroll arrival, removed after ~1.5s (e.g. `ring` or `bg-yellow-100` flash)
- Markdown link format: `[↩ Speaker, HH:MM AM/PM](#message-<turn_id>)`

#### Acceptance Criteria

- [ ] Every message in the thread has an anchor `id="message-<turn_id>"` on its DOM element
- [ ] Hovering a message on desktop reveals a reference/link action button
- [ ] Long-pressing a message on mobile reveals a reference action menu
- [ ] Clicking/tapping reference inserts a formatted markdown link into the composer (not a blockquote)
- [ ] Composer gains focus automatically after reference is inserted
- [ ] Reference link includes speaker label and timestamp
- [ ] Clicking a reference link in a sent message smooth-scrolls to the original message
- [ ] The original message briefly highlights on arrival (CSS animation)
- [ ] Works for both user messages and assistant messages
- [ ] No backend changes required for initial implementation

---

## Future Items (Backlog)

Items to consider adding to this story as they are identified:

- Message copy button (copy raw markdown to clipboard)
- Message retry / regenerate button (user messages only)
- Smooth scroll-to-bottom button when user has scrolled up mid-conversation
- Timestamp display toggle (always-on vs. hover-reveal)

---

## Human Review Instructions

*To be filled in after coding is complete (Step 5 of AGENT_WORKFLOW).*

---

## Approval

- [ ] **Implementation plan approved**
- [ ] **Coding complete**
- [ ] **Human review approved**
