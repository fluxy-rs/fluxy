import type { VariantProps } from "class-variance-authority";
import type { BaseProps, PolymorphicProps } from "../../types";
import type { attachmentVariants } from "./Attachment.styles";

export type AttachmentState = "idle" | "uploading" | "processing" | "error" | "done";

export type AttachmentProps = PolymorphicProps<
  "div",
  BaseProps &
    VariantProps<typeof attachmentVariants> & {
      state?: AttachmentState;
    },
  false
>;
