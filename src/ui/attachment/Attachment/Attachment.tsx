import { mergeProps, splitProps } from "solid-js";
import { clsx } from "../../utils";
import { attachmentVariants } from "./Attachment.styles";
import type { AttachmentProps } from "./Attachment.types";

/**
 */
export const Attachment = (props: AttachmentProps) => {
  const merged = mergeProps(
    {
      state: "done",
      size: "default",
      orientation: "horizontal",
    } as const,
    props,
  );

  const [local, rest] = splitProps(merged, ["state", "size", "orientation", "class", "classList"]);

  return (
    <div
      {...rest}
      data-slot="attachment"
      data-state={local.state}
      data-size={local.size}
      data-orientation={local.orientation}
      class={clsx(
        attachmentVariants({
          size: local.size,
          orientation: local.orientation,
        }),
        local.class,
      )}
      classList={local.classList}
    />
  );
};
