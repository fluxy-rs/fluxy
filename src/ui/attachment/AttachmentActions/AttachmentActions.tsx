import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import { attachmentActionsClasses } from "./AttachmentActions.styles";
import type { AttachmentActionsProps } from "./AttachmentActions.types";

export const AttachmentActions = (props: AttachmentActionsProps) => {
  const [local, rest] = splitProps(props, ["class", "classList"]);

  return (
    <div
      {...rest}
      data-slot="attachment-actions"
      class={clsx(attachmentActionsClasses, local.class)}
      classList={local.classList}
    />
  );
};
