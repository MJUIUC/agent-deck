import { create } from "zustand";

export type Palette =
  | "olive"
  | "slate"
  | "midnight"
  | "rose"
  | "forest"
  | "ember"
  | "ocean"
  | "copper"
  | "sakura"
  | "noir";

export type Mode = "system" | "light" | "dark";

interface ThemeStore {
  palette: Palette;
  mode: Mode;
  setPalette: (p: Palette) => void;
  setMode: (m: Mode) => void;
}

const STORAGE_KEY = "agent-deck:theme";

function loadStored(): { palette: Palette; mode: Mode } {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw);
      return {
        palette: parsed.palette ?? "olive",
        mode: parsed.mode ?? "system",
      };
    }
  } catch {
    // ignore parse/storage errors
  }
  return { palette: "olive", mode: "system" };
}

function persist(patch: Partial<{ palette: Palette; mode: Mode }>) {
  try {
    const current = loadStored();
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ ...current, ...patch }));
  } catch {
    // ignore storage errors
  }
}

export const useThemeStore = create<ThemeStore>(() => {
  const stored = loadStored();
  return {
    palette: stored.palette,
    mode: stored.mode,
    setPalette: (palette) => {
      useThemeStore.setState({ palette });
      persist({ palette });
    },
    setMode: (mode) => {
      useThemeStore.setState({ mode });
      persist({ mode });
    },
  };
});
