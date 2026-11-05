import type { VariantProps } from "class-variance-authority";
import type { BaseProps, PolymorphicProps } from "../../types";
import type { attachmentMediaVariants } from "./AttachmentMedia.styles";

export type AttachmentMediaProps = PolymorphicProps<
  "div",
  BaseProps & VariantProps<typeof attachmentMediaVariants>,
  false
>;
