import type { Accessor } from "solid-js";

/**
 *
 */
export const AT_EDGE_TOLERANCE = 0.5;

/**
 */
export function scrollableData(
  start: Accessor<boolean>,
  end: Accessor<boolean>,
): string | undefined {
  const tokens: string[] = [];
  if (start()) tokens.push("start");
  if (end()) tokens.push("end");
  return tokens.length > 0 ? tokens.join(" ") : undefined;
}
