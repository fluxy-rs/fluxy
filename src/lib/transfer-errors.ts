import { catalogs, type Locale } from "../i18n";

/**
 * Failure reasons for the stable protocol error codes.
 *
 * The catalog guard in `src/i18n/catalog.test.ts` reads `ErrorCode::as_str`
 * from `crates/fluxy-core/src/error.rs` and fails the build when a code has no
 * `transfers.reason.<code>` copy in either locale, so the two lists cannot
 * drift and an unknown code from a future peer degrades to the caller's
 * fallback instead of a raw key.
 */
export function failureReasonKey(code: string | null | undefined): string | null {
  if (!code) {
    return null;
  }
  const key = `transfers.reason.${code}`;
  return key in catalogs["zh-CN"] ? key : null;
}

/** The localized reason for a failure code, or `fallback` when it is unknown. */
export function failureReason(
  code: string | null | undefined,
  locale: Locale,
  fallback: string,
): string {
  const key = failureReasonKey(code);
  if (!key) {
    return fallback;
  }
  return catalogs[locale][key] ?? catalogs["zh-CN"][key] ?? fallback;
}
