import type { ValidComponent } from "solid-js";
import type { PolymorphicProps } from "../../types";

export type AttachmentTriggerProps<T extends ValidComponent = "button"> = PolymorphicProps<
  T,
  { type?: "button" | "submit" | "reset" }
>;
