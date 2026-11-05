import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import { attachmentTitleClasses } from "./AttachmentTitle.styles";
import type { AttachmentTitleProps } from "./AttachmentTitle.types";

export const AttachmentTitle = (props: AttachmentTitleProps) => {
  const [local, rest] = splitProps(props, ["class", "classList"]);

  return (
    <span
      {...rest}
      data-slot="attachment-title"
      class={clsx(attachmentTitleClasses, local.class)}
      classList={local.classList}
    />
  );
};
