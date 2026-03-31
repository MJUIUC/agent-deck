// ─── usePlatform ──────────────────────────────────────────────────────────────

/**
 * usePlatform — reads navigator.userAgent once and returns the platform.
 *
 * Returns:
 *   "ios"     — iPhone, iPad, or iPod
 *   "android" — Android device
 *   "other"   — desktop or unrecognised agent
 *
 * No reactive updates are needed because the user-agent never changes within
 * a session.
 */

type Platform = "ios" | "android" | "other";

function detectPlatform(): Platform {
  const ua = navigator.userAgent;
  if (/iphone|ipad|ipod/i.test(ua)) return "ios";
  if (/android/i.test(ua)) return "android";
  return "other";
}

export function usePlatform(): Platform {
  return detectPlatform();
}
