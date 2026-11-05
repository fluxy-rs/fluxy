import type { BaseProps, PolymorphicProps } from "../../types";

export type ScrollBarProps = PolymorphicProps<
  "div",
  BaseProps & { orientation?: "horizontal" | "vertical" },
  false
>;
