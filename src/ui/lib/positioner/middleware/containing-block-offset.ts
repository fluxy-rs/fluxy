import type { Middleware } from "../types";

const TRANSFORM_STYLE_PROPS = ["transform", "perspective", "filter", "backdropFilter"] as const;

function establishesContainingBlock(style: CSSStyleDeclaration): boolean {
  for (const prop of TRANSFORM_STYLE_PROPS) {
    const value = style[prop as keyof CSSStyleDeclaration] as string | undefined;
    if (value && value !== "none") return true;
  }
  const willChange = style.willChange || "";
  if (/transform|perspective|filter/.test(willChange)) return true;
  const contain = style.contain || "";
  if (/paint|layout|strict|content/.test(contain)) return true;
  return false;
}

/**
 *
 *
 *
 *
 */
export function containingBlockOffset(): Middleware {
  return {
    name: "containingBlockOffset",
    fn(state) {
      const floatingEl = state.elements.floating;
      let node: Element | null = floatingEl.parentElement;
      let offsetX = 0;
      let offsetY = 0;
      let foundAncestor: Element | null = null;

      while (node && node !== document.documentElement) {
        const style = getComputedStyle(node);
        if (establishesContainingBlock(style)) {
          const rect = node.getBoundingClientRect();
          offsetX = rect.x;
          offsetY = rect.y;
          foundAncestor = node;
          break;
        }
        node = node.parentElement;
      }

      if (!foundAncestor || (offsetX === 0 && offsetY === 0)) {
        return { data: { ancestor: null } };
      }

      return {
        x: state.x - offsetX,
        y: state.y - offsetY,
        data: { ancestor: foundAncestor, offsetX, offsetY },
      };
    },
  };
}
