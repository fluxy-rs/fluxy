/**
 *
 */

function isAnchor(element: HTMLElement): boolean {
  return element.dataset.scrollAnchor === "true";
}

export function firstAnchorFrom(list: HTMLElement[], from: number): HTMLElement | null {
  for (let index = from; index < list.length; index += 1) {
    const element = list[index];
    if (element && isAnchor(element)) return element;
  }
  return null;
}

export function firstUnhandledAnchor(
  list: HTMLElement[],
  handled: WeakSet<HTMLElement>,
): HTMLElement | null {
  return list.find((element) => isAnchor(element) && !handled.has(element)) ?? null;
}

export function hasMultipleAnchorsFrom(list: HTMLElement[], from: number): boolean {
  let count = 0;
  for (let index = from; index < list.length; index += 1) {
    if (list[index] && isAnchor(list[index])) {
      count += 1;
      if (count > 1) return true;
    }
  }
  return false;
}
