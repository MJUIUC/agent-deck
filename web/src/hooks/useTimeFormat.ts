import { useMemo } from "react";

/**
 * Format a timestamp for display in the thread list.
 * - Same day → "2:34 PM"
 * - Yesterday → "Yesterday"
 * - Within 7 days → day name ("Mon", "Tue", etc.)
 * - Older → "MM/DD/YY"
 */
export function formatThreadTime(isoString: string): string {
  const date = new Date(isoString);
  if (isNaN(date.getTime())) return "";

  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const startOfYesterday = new Date(startOfToday.getTime() - 86400000);
  const sevenDaysAgo = new Date(startOfToday.getTime() - 6 * 86400000);

  if (date >= startOfToday) {
    // Today → time
    return date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
  } else if (date >= startOfYesterday) {
    return "Yesterday";
  } else if (date >= sevenDaysAgo) {
    // Within last 7 days → short day name
    return date.toLocaleDateString([], { weekday: "short" });
  } else {
    // Older → short date
    return date.toLocaleDateString([], {
      month: "numeric",
      day: "numeric",
      year: "2-digit",
    });
  }
}

/**
 * Format a message timestamp for display in the chat view.
 * Returns e.g. "Aldous · 10:32 AM"
 */
export function formatMessageTime(isoString: string): string {
  const date = new Date(isoString);
  if (isNaN(date.getTime())) return "";
  return date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

/**
 * Format a date for the date divider between message groups.
 * - Today → "Today"
 * - Yesterday → "Yesterday"
 * - Older → "Mon, Jan 6"
 */
export function formatDateDivider(isoString: string): string {
  const date = new Date(isoString);
  if (isNaN(date.getTime())) return "";

  const now = new Date();
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const startOfYesterday = new Date(startOfToday.getTime() - 86400000);

  if (date >= startOfToday) return "Today";
  if (date >= startOfYesterday) return "Yesterday";

  return date.toLocaleDateString([], { weekday: "short", month: "short", day: "numeric" });
}

/**
 * Group messages by calendar day.
 * Returns an array of { dateLabel, items } groups, oldest first.
 */
export function groupByDate<T extends { created_at: string }>(
  items: T[]
): { dateLabel: string; items: T[] }[] {
  const groups: { dateLabel: string; items: T[] }[] = [];
  let currentLabel = "";

  for (const item of items) {
    const label = formatDateDivider(item.created_at);
    if (label !== currentLabel) {
      currentLabel = label;
      groups.push({ dateLabel: label, items: [] });
    }
    groups[groups.length - 1].items.push(item);
  }

  return groups;
}

/**
 * Hook wrapper for use in React components.
 * Returns formatters memoized once (they have no deps).
 */
export function useTimeFormat() {
  return useMemo(
    () => ({
      formatThreadTime,
      formatMessageTime,
      formatDateDivider,
      groupByDate,
    }),
    []
  );
}
