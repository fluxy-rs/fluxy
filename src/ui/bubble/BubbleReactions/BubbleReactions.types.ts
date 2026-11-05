import type { VariantProps } from "class-variance-authority";
import type { BaseProps, PolymorphicProps } from "../../types";
import type { bubbleReactionsVariants } from "./BubbleReactions.styles";

export type BubbleReactionsProps = PolymorphicProps<
  "div",
  BaseProps & VariantProps<typeof bubbleReactionsVariants>,
  false
>;
