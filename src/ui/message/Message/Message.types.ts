import type { BaseProps, PolymorphicProps } from "../../types";

export type MessageAlign = "start" | "end";

export type MessageProps = PolymorphicProps<"div", BaseProps & { align?: MessageAlign }, false>;
