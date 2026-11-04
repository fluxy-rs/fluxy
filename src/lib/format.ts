import type { AppEventEnvelope } from "../bindings";

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB", "PB"] as const;

/** Human readable byte size using binary multiples (KiB shown as KB). */
export function formatBytes(bytes: number, fractionDigits = 1): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), BYTE_UNITS.length - 1);
  const value = bytes / 1024 ** exponent;
  const digits = exponent === 0 ? 0 : value >= 100 ? 0 : value >= 10 ? 1 : fractionDigits;
  return `${value.toFixed(digits)} ${BYTE_UNITS[exponent]}`;
}

/** Transfer rate. Returns an em dash when the rate is unknown. */
export function formatSpeed(bytesPerSecond: number): string {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) {
    return "—";
  }
  return `${formatBytes(bytesPerSecond)}/s`;
}

/** Whole percent, clamped to 0..100. */
export function formatPercent(done: number, total: number): number {
  if (!Number.isFinite(total) || total <= 0) {
    return 0;
  }
  return Math.max(0, Math.min(100, Math.round((done / total) * 100)));
}

/** Seconds left at the current rate, or null when it cannot be estimated. */
export function etaSeconds(done: number, total: number, bytesPerSecond: number): number | null {
  if (
    !Number.isFinite(bytesPerSecond) ||
    bytesPerSecond <= 0 ||
    !Number.isFinite(total) ||
    total <= 0
  ) {
    return null;
  }
  return Math.max(0, total - done) / bytesPerSecond;
}

export function formatEta(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds) || seconds <= 0) {
    return "—";
  }
  if (seconds < 60) {
    return `${Math.ceil(seconds)}s`;
  }
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) {
    return `${minutes}m ${Math.round(seconds % 60)}s`;
  }
  const hours = Math.floor(minutes / 60);
  return `${hours}h ${minutes % 60}m`;
}

export function shortId(id: string, length = 12): string {
  return id.length > length ? `${id.slice(0, length)}…` : id;
}

export function eventName(envelope: AppEventEnvelope): string {
  const event = envelope.event;
  if (event.category === "error") {
    return `error.${event.error.code}`;
  }
  return `${event.category}.${event.data.type}`;
}

export function formatTime(ms: number, locale: string): string {
  return new Date(ms).toLocaleTimeString(locale, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function formatClock(ms: number, locale: string): string {
  return new Date(ms).toLocaleTimeString(locale, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatDate(ms: number, locale: string): string {
  return new Date(ms).toLocaleDateString(locale, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // WebView clipboard permissions vary; fall back to a hidden selection.
    try {
      const area = document.createElement("textarea");
      area.value = text;
      area.setAttribute("readonly", "true");
      area.style.position = "fixed";
      area.style.opacity = "0";
      document.body.appendChild(area);
      area.select();
      const copied = document.execCommand("copy");
      area.remove();
      return copied;
    } catch {
      return false;
    }
  }
}

export function errorDetails(details: unknown): string {
  try {
    return JSON.stringify(details, null, 2);
  } catch {
    return String(details);
  }
}
