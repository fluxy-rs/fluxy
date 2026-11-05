import type { DrawerSwipeDirection } from "../Drawer/Drawer.types";

export function closedTransform(direction: DrawerSwipeDirection): string {
  switch (direction) {
    case "down":
      return "translate3d(0, 100%, 0)";
    case "up":
      return "translate3d(0, -100%, 0)";
    case "left":
      return "translate3d(-100%, 0, 0)";
    case "right":
      return "translate3d(100%, 0, 0)";
  }
}

export function isHorizontal(direction: DrawerSwipeDirection): boolean {
  return direction === "left" || direction === "right";
}

const axisStyles: Record<DrawerSwipeDirection, string> = {
  down: "inset-x-0 bottom-0 max-h-[calc(100dvh-6rem)] pb-safe",
  up: "inset-x-0 top-0 max-h-[calc(100dvh-6rem)] pt-safe",
  left: "inset-y-0 left-0 w-3/4 sm:w-96 pl-safe",
  right: "inset-y-0 right-0 w-3/4 sm:w-96 pr-safe",
};

/**
 */
export const drawerPopupBase =
  "group/drawer-popup pointer-events-auto fixed z-50 m-(--drawer-inset,0.75rem) flex flex-col rounded-xl border bg-popover text-sm text-popover-foreground shadow-lg outline-none select-none data-[swipe-axis=x]:flex-row";

export function drawerPopupDirectionClass(direction: DrawerSwipeDirection) {
  return axisStyles[direction];
}
