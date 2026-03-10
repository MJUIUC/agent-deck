import React, { useState } from "react";
import { EMOJI_PALETTE, EMOJI_CATEGORIES } from "./shared";

// ─── EmojiBtn ─────────────────────────────────────────────────────────────────

export function EmojiBtn({
  emoji,
  selected,
  onClick,
}: {
  emoji: string;
  selected: boolean;
  onClick: () => void;
}) {
  const [hovered, setHovered] = useState(false);
  return (
    <button
      type="button"
      onClick={onClick}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      style={{
        width: 36,
        height: 36,
        borderRadius: 7,
        fontSize: 18,
        background: selected
          ? "var(--accent-muted)"
          : hovered
            ? "var(--bg-elevated)"
            : "var(--bg-tertiary)",
        border: `2px solid ${selected ? "var(--accent-primary)" : "transparent"}`,
        cursor: "pointer",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        transition: "border-color 0.15s, background 0.15s",
        flexShrink: 0,
      }}
    >
      {emoji}
    </button>
  );
}

// ─── EmojiPicker ─────────────────────────────────────────────────────────────

interface EmojiPickerProps {
  /** The currently selected emoji. */
  value: string;
  /** Called when the user picks a new emoji. */
  onChange: (emoji: string) => void;
}

/**
 * Renders the quick emoji palette row (+ expand button) and, when expanded,
 * the full picker with search, category tabs, and an emoji grid.
 *
 * Completely self-contained — owns its open/search/category state.
 */
export function EmojiPicker({ value, onChange }: EmojiPickerProps) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [categoryIndex, setCategoryIndex] = useState(0);

  const selectEmoji = (e: string) => {
    onChange(e);
    setSearch("");
    setOpen(false);
  };

  // Emojis shown in the grid when the picker is open
  const gridEmojis = search
    ? EMOJI_CATEGORIES.flatMap((c) => c.emojis).filter((e) =>
        e.includes(search.trim()),
      )
    : EMOJI_CATEGORIES[categoryIndex].emojis;

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      {/* ── Quick palette row ── */}
      <div
        style={{
          display: "flex",
          gap: 8,
          flexWrap: "wrap",
          marginTop: 4,
          alignItems: "center",
        }}
      >
        {/* If the selected emoji is not in the palette, show it as a "custom" chip */}
        {!EMOJI_PALETTE.includes(value) && (
          <EmojiBtn
            key="custom"
            emoji={value}
            selected
            onClick={() => setOpen((o) => !o)}
          />
        )}

        {EMOJI_PALETTE.map((e) => (
          <EmojiBtn
            key={e}
            emoji={e}
            selected={value === e}
            onClick={() => selectEmoji(e)}
          />
        ))}

        {/* Expand / collapse button */}
        <button
          type="button"
          title="Browse all emojis"
          onClick={() => setOpen((o) => !o)}
          style={{
            width: 36,
            height: 36,
            borderRadius: 7,
            fontSize: 16,
            background: open ? "var(--accent-primary)" : "var(--bg-tertiary)",
            border: `2px solid ${open ? "var(--accent-primary)" : "transparent"}`,
            cursor: "pointer",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: open ? "var(--text-inverse)" : "var(--text-secondary)",
            transition: "all 0.15s",
            flexShrink: 0,
            fontFamily: "inherit",
          }}
        >
          {open ? "✕" : "···"}
        </button>
      </div>

      {/* ── Full picker panel ── */}
      {open && (
        <div
          style={{
            marginTop: 10,
            background: "var(--bg-primary)",
            border: "1px solid var(--border-default)",
            borderRadius: 10,
            padding: "10px 12px",
            display: "flex",
            flexDirection: "column",
            gap: 8,
          }}
        >
          {/* Search input */}
          <input
            type="text"
            placeholder="Type or paste any emoji…"
            value={search}
            autoFocus
            onChange={(e) => setSearch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                const val = e.currentTarget.value.trim();
                if (!val) return;
                const seg = [...new Intl.Segmenter().segment(val)];
                if (seg.length > 0) {
                  selectEmoji(seg[0].segment);
                  setSearch("");
                }
              }
            }}
            style={{
              background: "var(--bg-secondary)",
              border: "1px solid var(--border-subtle)",
              borderRadius: 7,
              padding: "7px 10px",
              color: "var(--text-primary)",
              fontSize: 13,
              fontFamily: "inherit",
              outline: "none",
              width: "100%",
              boxSizing: "border-box",
            }}
          />

          {/* Category tabs — hidden while searching */}
          {!search && (
            <div style={{ display: "flex", gap: 4, flexWrap: "wrap" }}>
              {EMOJI_CATEGORIES.map((cat, i) => (
                <button
                  key={cat.label}
                  type="button"
                  onClick={() => setCategoryIndex(i)}
                  style={{
                    padding: "3px 8px",
                    borderRadius: 5,
                    fontSize: 11,
                    fontWeight: 500,
                    cursor: "pointer",
                    border: "none",
                    background:
                      categoryIndex === i
                        ? "var(--accent-primary)"
                        : "var(--bg-elevated)",
                    color:
                      categoryIndex === i
                        ? "var(--text-inverse)"
                        : "var(--text-secondary)",
                    fontFamily: "inherit",
                    transition: "background 0.12s",
                  }}
                >
                  {cat.label}
                </button>
              ))}
            </div>
          )}

          {/* Emoji grid */}
          <div
            style={{
              display: "flex",
              flexWrap: "wrap",
              gap: 4,
              maxHeight: 180,
              overflowY: "auto",
            }}
          >
            {gridEmojis.map((e) => (
              <EmojiBtn
                key={e}
                emoji={e}
                selected={value === e}
                onClick={() => selectEmoji(e)}
              />
            ))}

            {search && gridEmojis.length === 0 && (
              <div
                style={{
                  fontSize: 12,
                  color: "var(--text-tertiary)",
                  padding: "8px 4px",
                }}
              >
                No matches — press Enter to use "{search.trim()}" directly.
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
