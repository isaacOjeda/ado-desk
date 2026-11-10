// Platform detection for UI differences (shortcut labels, title bar spacing).

export type Platform = "mac" | "windows" | "other";

export function detectPlatform(userAgent: string = navigator.userAgent): Platform {
  if (/Macintosh|Mac OS X/i.test(userAgent)) return "mac";
  if (/Windows/i.test(userAgent)) return "windows";
  return "other";
}

export const PLATFORM: Platform = detectPlatform();
export const IS_MAC = PLATFORM === "mac";

/** Prefix for a Cmd/Ctrl shortcut label: `⌘K` on macOS, `Ctrl+K` elsewhere. */
export const MOD_KEY = IS_MAC ? "⌘" : "Ctrl+";

/** Minimal shape of User-Agent Client Hints (not in the ES2020 DOM typings). */
interface UserAgentData {
  getHighEntropyValues(hints: string[]): Promise<{ platformVersion?: string }>;
}

let windows11 = false;

/**
 * Windows 11 reports a `platformVersion` major of 13 or higher through Client Hints; the user
 * agent string says "Windows NT 10.0" on both Windows 10 and 11. Call once before rendering.
 */
export async function initPlatform(): Promise<void> {
  if (PLATFORM !== "windows") return;
  try {
    const data = (navigator as Navigator & { userAgentData?: UserAgentData }).userAgentData;
    const { platformVersion } = (await data?.getHighEntropyValues(["platformVersion"])) ?? {};
    windows11 = Number.parseInt(platformVersion ?? "", 10) >= 13;
  } catch {
    windows11 = false;
  }
}

/** Whether the OS draws a native material behind the window: vibrancy on macOS, Mica on Windows 11. */
export function hasNativeGlass(): boolean {
  return IS_MAC || windows11;
}
