/**
 *
 */

export interface PaddingBox {
  start: number;
  end: number;
}

export function parsePx(value: string): number {
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

/**
 */
export function paddingBox(element: HTMLElement): PaddingBox {
  const style = window.getComputedStyle(element);
  return {
    start: parsePx(style.paddingBlockStart || style.paddingTop),
    end: parsePx(style.paddingBlockEnd || style.paddingBottom),
  };
}

/**
 */
export function rowGap(element: HTMLElement | null): number {
  if (!element) return 0;
  const style = window.getComputedStyle(element);
  return parsePx(style.rowGap === "normal" ? style.gap : style.rowGap);
}
