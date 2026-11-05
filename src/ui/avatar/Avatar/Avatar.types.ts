import type { BaseProps, PolymorphicProps } from "../../types";

export type AvatarProps = PolymorphicProps<
  "span",
  BaseProps & {
    size?: "md" | "sm" | "lg";
  },
  false
>;
