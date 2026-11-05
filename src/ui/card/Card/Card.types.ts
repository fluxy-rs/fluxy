import type { BaseProps, PolymorphicProps } from "../../types";

interface BaseCardProps extends BaseProps {
  size?: "md" | "sm";
}

export type CardProps = PolymorphicProps<"div", BaseCardProps, false>;
