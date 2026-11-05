import { mergeProps, splitProps } from "solid-js";
import { clsx } from "../../utils";
import { attachmentMediaVariants } from "./AttachmentMedia.styles";
import type { AttachmentMediaProps } from "./AttachmentMedia.types";

export const AttachmentMedia = (props: AttachmentMediaProps) => {
  const merged = mergeProps({ variant: "icon" } as const, props);

  const [local, rest] = splitProps(merged, ["variant", "class", "classList"]);

  return (
    <div
      {...rest}
      data-slot="attachment-media"
      data-variant={local.variant}
      class={clsx(attachmentMediaVariants({ variant: local.variant }), local.class)}
      classList={local.classList}
    />
  );
};
