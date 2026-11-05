import type { ValidComponent } from "solid-js";
import type { BaseProps, PolymorphicProps } from "../../types";

export type BubbleContentProps<T extends ValidComponent = "div"> = PolymorphicProps<T, BaseProps>;
