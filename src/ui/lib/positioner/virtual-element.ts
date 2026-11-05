import type { VirtualElement } from "./types";

export interface PointOptions {
  x: number;
  y: number;
  contextElement?: Element;
}

/**
 *
 * @example
 * ```ts
 * const [point, setPoint] = createSignal<VirtualElement>();
 * el.addEventListener('contextmenu', (e) => {
 *   e.preventDefault();
 *   setPoint(createVirtualElement({ x: e.clientX, y: e.clientY, contextElement: el }));
 * });
 * ```
 */
export function createVirtualElement(point: PointOptions): VirtualElement {
  return {
    contextElement: point.contextElement,
    getBoundingClientRect() {
      return { x: point.x, y: point.y, width: 0, height: 0 };
    },
  };
}
