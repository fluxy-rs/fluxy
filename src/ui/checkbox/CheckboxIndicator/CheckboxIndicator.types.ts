import type { BaseProps, PolymorphicProps } from "../../types";

export type CheckboxIndicatorProps = Omit<PolymorphicProps<"span", BaseProps, false>, "children">;
