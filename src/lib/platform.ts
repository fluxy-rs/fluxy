/**
 * Short, human-readable platform labels for the device list.
 *
 * A glyph was the first attempt, but lucide has no brand marks and every
 * device-shaped icon collapses into the same rounded rectangle at list size.
 * A word is unambiguous at 11px.
 */
const LABELS: Record<string, string> = {
  android: "Android",
  ios: "iOS",
  macos: "macOS",
  windows: "Windows",
  linux: "Linux",
};

export function platformLabel(platform: string): string {
  const key = platform.trim().toLowerCase();
  return LABELS[key] ?? platform;
}
