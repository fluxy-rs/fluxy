import type { Accessor } from "solid-js";

export interface ScrollArrowsProps {
  /** 滚动容器 accessor；组件内部据此计算上下边缘状态并驱动悬停滚动 */
  target: Accessor<HTMLElement | undefined>;
  /**
   * 指针停在滚动容器上/下边缘带时是否持续滚动，默认 **true**。
   *
   * 注意：悬停滚动是在**容器**上按指针坐标驱动的（箭头本身始终
   * `pointer-events-none`），因此不会抢走边缘列表项的点击；
   * 指针按下、滚轮、键盘、离开容器都会立即停止。
   */
  hoverScroll?: boolean;
  /** 两个箭头共用的额外 class */
  class?: string;
  /** 顶部箭头额外 class(圆角等) */
  upClass?: string;
  /** 底部箭头额外 class(圆角等) */
  downClass?: string;
}

export interface ScrollArrowButtonProps {
  direction: "up" | "down";
  visible: boolean;
  class?: string;
}
