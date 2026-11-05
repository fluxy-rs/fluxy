import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { AttachmentContentProps } from "./AttachmentContent.types";

export const AttachmentContent = (props: AttachmentContentProps) => {
  const [local, rest] = splitProps(props, ["class", "classList"]);

  return (
    <div
      {...rest}
      data-slot="attachment-content"
      class={clsx(
        "max-w-full min-w-0 flex-1 leading-tight group-data-[orientation=vertical]/attachment:px-1",
        local.class,
      )}
      classList={local.classList}
    />
  );
};
