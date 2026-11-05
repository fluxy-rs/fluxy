import type { VariantProps } from "class-variance-authority";
import type { BaseProps, PolymorphicProps } from "../../types";
import type { bubbleVariants } from "./Bubble.styles";

export type BubbleAlign = "start" | "end";

export type BubbleProps = PolymorphicProps<
  "div",
  BaseProps &
    VariantProps<typeof bubbleVariants> & {
      align?: BubbleAlign;
    },
  false
>;
