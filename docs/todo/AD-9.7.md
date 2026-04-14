# AD-9.7 — Long-Running Tool Progress Phrases

**Story:** 9.7 — Long-running tool progress phrases
**Branch:** `feature/phase9-tool-progress-phrases`
**Phase doc reference:** `docs/PLAN/PLAN_3.md` §Phase 9

---

## Summary

When the agent enters any processing phase — reasoning or tool execution — and no meaningful event arrives within 4 seconds, a rotating progress phrase is displayed to indicate work is ongoing. Phrases escalate in tone across four tiers as elapsed time grows. A new phrase set is randomly selected each time a long-running phase fires, keeping the experience fresh across multiple cycles within the same thread. Phrases disappear immediately when the bounding end event is received.

This requires **both backend and frontend changes**:
- **Backend:** Two new SSE events (`ReasoningStart`, `ReasoningEnd`) bracket each LLM generation pass, giving the frontend clean boundaries around all three dead zone types.
- **Frontend:** Timer-based phrase escalation logic, driven by matching start/end event pairs.

---

## Motivation

There are three distinct phases in an agent turn where the UI can go silent:

| Phase | Events | Dead zone |
|---|---|---|
| Pre-tool reasoning | `ReasoningStart` → `ReasoningEnd` | LLM generating tokens before deciding on a tool call — token stream may be sparse or absent |
| Tool execution | `ToolStart` → `ToolRoundComplete` | MCP tool is running — no output until complete |
| Post-tool reasoning | `ReasoningStart` → `ReasoningEnd` | LLM processing tool results, deciding next step — may produce no visible tokens |

Previously, the server emitted no events around LLM generation passes, meaning inter-round reasoning was invisible to the frontend. The token stream alone is not sufficient — it can be absent (e.g. during silent reasoning) or very sparse, providing no reliable "are we working?" signal.

Adding `ReasoningStart` / `ReasoningEnd` gives every phase clean boundaries, and allows the same timer/phrase mechanism to cover all three dead zones with a single unified approach.

---

## New SSE Events

### `ReasoningStart`

Emitted immediately before the LLM begins generating tokens for a new pass.

```json
{ "event": "ReasoningStart", "data": { "turn_id": "<uuid>" } }
```

- Fires at the **start of every LLM generation pass** — both the initial reasoning pass and each post-tool-result reasoning pass.
- Acts as a natural heartbeat: if the frontend sees `ReasoningStart`, it knows the LLM loop is alive.

### `ReasoningEnd`

Emitted immediately after the LLM finishes generating and the outcome has been determined (tool call dispatched, or final response complete).

```json
{ "event": "ReasoningEnd", "data": { "turn_id": "<uuid>", "outcome": "tool_call" | "final_response" } }
```

- `outcome: "tool_call"` — a tool call was extracted; `ToolStart` will follow.
- `outcome: "final_response"` — the turn is complete; `ChatSegment` will follow (or has already been streaming).

### Updated SSE Event Sequence

```
ReasoningStart
  → Token stream (sparse or absent during silent reasoning)
ReasoningEnd  { outcome: "tool_call" }

ToolStart
  → MCP executing...
ToolRoundComplete

ReasoningStart
  → Token stream (processing tool result)
ReasoningEnd  { outcome: "tool_call" }

ToolStart
  ...
ToolRoundComplete

ReasoningStart
  → Token stream (final answer)
ReasoningEnd  { outcome: "final_response" }

ChatSegment
```

---

## Behaviour Specification

### Dead Zone Coverage

| Event pair | Dead zone covered |
|---|---|
| `ReasoningStart` → `ReasoningEnd` | Pre-tool reasoning silence |
| `ToolStart` → `ToolRoundComplete` | Tool execution silence |
| `ReasoningStart` → `ReasoningEnd` | Post-tool reasoning silence |

Each pair uses the same timer mechanism: if the end event does not arrive within **4 seconds** of the start event, phrase escalation begins.

### Trigger

The phrase display activates **only** when:
1. A start event (`ReasoningStart` or `ToolStart`) is received, AND
2. The corresponding end event (`ReasoningEnd` or `ToolRoundComplete`) has **not** arrived within **4 seconds**

### Escalation Tiers

| Elapsed time | Tier |
|---|---|
| ≥ 4s | Tier 1 |
| ≥ 10s | Tier 2 |
| ≥ 20s | Tier 3 |
| ≥ 45s | Tier 4 |

### Phrase Set Selection

- A phrase set is **randomly selected at the moment the tier-1 timer fires** (not at the start event)
- The same set is used for all tier escalations within that phase
- A **new random set** is selected independently for each subsequent long-running phase
- No guarantee of non-repetition across phases — pure random pick from the pool

### Placement

**During reasoning phases (`ReasoningStart` / `ReasoningEnd`):**
Phrases appear in a dedicated reasoning status area — e.g. a subtle banner or status line below the token stream, visually distinct from message content.

**During tool execution phases (`ToolStart` / `ToolRoundComplete`):**
Phrases appear at the **bottom of the tool call message bubble**, below existing tool content (name, arguments, partial output).

In both cases: italicised, muted/secondary text colour, soft pulsing animation.

### Dismissal

The phrase disappears immediately when the corresponding end event is received. No fade — instant clear.

---

## Phrase Sets

Each set contains exactly 4 phrases, one per tier. Sets are designed to feel like distinct "voices" so the experience varies meaningfully across phases.

```ts
const PROGRESS_PHRASE_SETS: [string, string, string, string][] = [
  // Set A — The Thinker
  ["Thinking...", "Still thinking...", "Really thinking...", "This is what thinking looks like."],

  // Set B — The Professional
  ["Working on it...", "Digging in...", "Getting to the bottom of this...", "Thoroughness takes time."],

  // Set C — The Honest One
  ["One moment...", "Still going...", "This is taking longer than expected...", "Genuinely not stuck, just slow."],

  // Set D — The Dramatic
  ["On it...", "Deep in the weeds...", "Navigating some complexity here...", "This one has layers."],

  // Set E — The Casual
  ["Bear with me...", "Still here, still working...", "Not forgot about you...", "Worth the wait, probably."],

  // Set F — The Self-Aware
  ["Earning my keep...", "This requires actual effort...", "Not all tasks are easy, it turns out...", "You picked a hard one."],

  // Set G — The Understated
  ["...", "...still...", "...really still...", "...okay this is taking a minute."],

  // Set H — The Explorer
  ["Following the trail...", "Going further down...", "It's deep in here...", "Found something. Investigating."],

  // Set I — The Optimist
  ["Almost there...", "Getting warmer...", "Close now...", "Closer than I was, at least."],

  // Set J — The Engineer
  ["Processing...", "Still processing...", "Processing harder...", "This is a lot of processing."],

  // Set K — The Apologetic
  ["Sorry, just a sec...", "Still sorry, still a sec...", "Genuinely sorry about this...", "Deeply sorry. Truly."],

  // Set L — The Zen
  ["Patience...", "More patience...", "This is the work...", "The answer will arrive when it arrives."],

  // Set M — The Suspicious
  ["Looking into it...", "Looking deeper into it...", "There's definitely something here...", "Not saying what, but something."],

  // Set N — The Reassuring
  ["I've got this...", "Still got this...", "Maintaining my confidence...", "Confidence unchanged. Timeline unknown."],

  // Set O — The Weary
  ["On it...", "Still on it...", "Very much still on it...", "It is, perhaps, a lot."],
];
```

---

## Implementation Plan

### Backend Tasks

#### Task B1 — Emit `ReasoningStart` event

- File: whichever module drives the main LLM generation loop (likely `src/agent/runner.rs` or similar — confirm in codebase)
- Before calling the LLM for each generation pass, emit:
  ```rust
  SseEvent::ReasoningStart { turn_id }
  ```
- Must fire for **both** the initial reasoning pass and all subsequent post-tool-result passes.

#### Task B2 — Emit `ReasoningEnd` event

- In the same module, after the LLM generation pass completes and the outcome is known:
  ```rust
  SseEvent::ReasoningEnd { turn_id, outcome: ReasoningOutcome::ToolCall }
  // or
  SseEvent::ReasoningEnd { turn_id, outcome: ReasoningOutcome::FinalResponse }
  ```

#### Task B3 — Define new SSE event variants

- File: wherever `SseEvent` (or equivalent) is defined (likely `src/sse.rs` or `src/types.rs`)
- Add:
  ```rust
  ReasoningStart { turn_id: Uuid },
  ReasoningEnd { turn_id: Uuid, outcome: ReasoningOutcome },
  ```
- Add:
  ```rust
  pub enum ReasoningOutcome {
      ToolCall,
      FinalResponse,
  }
  ```
- Serialise `outcome` as `"tool_call"` / `"final_response"` in the SSE JSON payload.

### Frontend Tasks

#### Task F1 — Define the phrase set data structure

- File: `src/lib/progressPhrases.ts` (new file)
- Export the `PROGRESS_PHRASE_SETS` constant as typed above
- Export a helper:
  ```ts
  export function pickPhraseSet(): [string, string, string, string] {
    return PROGRESS_PHRASE_SETS[
      Math.floor(Math.random() * PROGRESS_PHRASE_SETS.length)
    ];
  }
  ```

#### Task F2 — Add phrase timer logic to the tool call bubble component

- File: tool call bubble component (likely `ToolCallMessage.tsx` or similar — confirm in codebase)
- Add local state:
  ```ts
  const [currentPhrase, setCurrentPhrase] = useState<string | null>(null);
  ```
- Drive off `isComplete` (derived from `ToolRoundComplete` received for this tool call's ID):
  ```ts
  useEffect(() => {
    if (isComplete) {
      setCurrentPhrase(null);
      return;
    }

    let phraseSet: [string, string, string, string] | null = null;

    const t1 = setTimeout(() => {
      phraseSet = pickPhraseSet();
      setCurrentPhrase(phraseSet[0]);
    }, 4_000);

    const t2 = setTimeout(() => {
      if (phraseSet) setCurrentPhrase(phraseSet[1]);
    }, 10_000);

    const t3 = setTimeout(() => {
      if (phraseSet) setCurrentPhrase(phraseSet[2]);
    }, 20_000);

    const t4 = setTimeout(() => {
      if (phraseSet) setCurrentPhrase(phraseSet[3]);
    }, 45_000);

    return () => {
      clearTimeout(t1); clearTimeout(t2); clearTimeout(t3); clearTimeout(t4);
      setCurrentPhrase(null);
    };
  }, [isComplete]);
  ```

#### Task F3 — Add phrase timer logic to the reasoning status component

- File: wherever inter-round reasoning state is tracked / rendered (confirm in codebase)
- Same timer logic as Task F2, but driven by `ReasoningStart` / `ReasoningEnd` events instead of `ToolStart` / `ToolRoundComplete`
- The `isReasoningComplete` flag resets each time a new `ReasoningStart` fires, ensuring each reasoning pass gets its own independent timer

#### Task F4 — Render phrases in both locations

**Tool call bubble:**
```tsx
{currentPhrase && (
  <p className="italic text-sm text-muted-foreground animate-pulse mt-2">
    {currentPhrase}
  </p>
)}
```

**Reasoning status area:**
```tsx
{reasoningPhrase && (
  <p className="italic text-sm text-muted-foreground animate-pulse mt-2">
    {reasoningPhrase}
  </p>
)}
```

#### Task F5 — Style the phrases

- Requirements for both locations:
  - Italicised
  - Muted/secondary text colour
  - Subtle pulse animation (opacity 100% → 60% → 100%, ~2s cycle)
  - If `animate-pulse` feels too distracting, a custom slower keyframe is preferred

### Schema changes

None beyond the new SSE event variants (no database changes).

### Parallelisation note

- **Task B3** (define new SSE variants) must be done first — unblocks B1, B2, and F3.
- **Task F1** (phrase data file) can be done independently in parallel with backend work.
- **Tasks B1, B2** can proceed in parallel after B3.
- **Tasks F2, F3, F4, F5** all touch UI components — do in a single pass after B3 is merged.

Recommended agent split:
- **Sub-agent A**: Task B3 + B1 + B2 — new SSE events, server-side emission
- **Sub-agent B**: Task F1 — `progressPhrases.ts`
- After A completes → **Sub-agent C**: Tasks F2, F3, F4, F5 — UI components + styles

---

## Acceptance Criteria

### Backend
- [ ] `ReasoningStart` is emitted before every LLM generation pass (initial and post-tool-result)
- [ ] `ReasoningEnd` is emitted after every LLM generation pass, with correct `outcome` value
- [ ] Both events include `turn_id` in the payload
- [ ] New SSE event variants serialise correctly as JSON

### Frontend
- [ ] No phrase appears when a tool call completes in under 4 seconds
- [ ] No phrase appears when a reasoning pass completes in under 4 seconds
- [ ] Tier 1 phrase appears after 4 seconds of no end event for an active phase
- [ ] Tier 2, 3, 4 phrases appear at 10s, 20s, 45s respectively, using the same phrase set selected at tier 1
- [ ] A different phrase set may be selected for each long-running phase within the same thread
- [ ] Tool call phrase appears at the bottom of the tool call bubble
- [ ] Reasoning phrase appears in the reasoning status area
- [ ] Phrases disappear immediately when the corresponding end event is received
- [ ] All 15 phrase sets are present and reachable by the random picker
- [ ] No regressions to existing tool bubble or token stream rendering
- [ ] Mobile and desktop layouts both display phrases correctly

---

## Human Review Instructions

*Leave blank until coding is complete.*

---

## Approval

- [ ] **Implementation plan approved**
- [ ] **Coding complete** — all acceptance criteria verified
- [ ] **Human review approved**
