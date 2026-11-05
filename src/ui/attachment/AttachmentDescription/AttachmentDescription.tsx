import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import { attachmentDescriptionClasses } from "./AttachmentDescription.styles";
import type { AttachmentDescriptionProps } from "./AttachmentDescription.types";

export const AttachmentDescription = (props: AttachmentDescriptionProps) => {
  const [local, rest] = splitProps(props, ["class", "classList"]);

  return (
    <span
      {...rest}
      data-slot="attachment-description"
      class={clsx(attachmentDescriptionClasses, local.class)}
      classList={local.classList}
    />
  );
};
