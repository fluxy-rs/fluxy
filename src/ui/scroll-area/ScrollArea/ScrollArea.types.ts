import type { BaseProps, PolymorphicProps } from "../../types";

export type ScrollAreaProps = PolymorphicProps<
  "div",
  BaseProps & {
    "aria-label"?: string;
    orientation?: "horizontal" | "vertical";
  },
  false
>;

export interface ScrollMetrics {
  /** 0–1 fraction of viewport / content */
  thumbRatio: number;
  /** 0–1 scroll position fraction */
  thumbOffset: number;
  /** Whether the axis is scrollable at all */
  scrollable: boolean;
}
