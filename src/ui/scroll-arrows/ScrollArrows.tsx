import type { JSX } from "solid-js";
import { clsx } from "../utils";
import { useScrollEdges } from "../hooks";
import { ScrollArrowButton } from "./ScrollArrowButton";
import { useHoverScroll } from "./useHoverScroll";
import type { ScrollArrowsProps } from "./ScrollArrows.types";

/**
 * 列表/菜单弹层通用的滚动提示：隐藏原生滚动条后，在上/下沿叠加箭头，
 * 提示该方向还有内容。放在滚动容器(定位祖先)内部即可，需要容器自身是定位元素。
 *
 * - 箭头本身始终是**装饰层**（`pointer-events-none`）：不抢边缘列表项的点击；
 * - 悬停滚动默认开启，但由 `useHoverScroll` 在滚动容器上按指针坐标驱动，
 *   因此"箭头显隐"与"是否继续滚动"互不依赖（见 DESIGN.md §3）；
 * - 需要纯装饰时传 `hoverScroll={false}`。
 */
export function ScrollArrows(props: ScrollArrowsProps): JSX.Element {
  const { canScrollUp, canScrollDown } = useScrollEdges(() => props.target());

  useHoverScroll({
    target: () => props.target(),
    canScrollUp,
    canScrollDown,
    enabled: () => props.hoverScroll ?? true,
  });

  return (
    <>
      <ScrollArrowButton
        direction="up"
        visible={canScrollUp()}
        class={clsx(props.class, props.upClass)}
      />
      <ScrollArrowButton
        direction="down"
        visible={canScrollDown()}
        class={clsx(props.class, props.downClass)}
      />
    </>
  );
}
