import { mergeProps } from "solid-js";
import { Button } from "../../button";
import type { AttachmentActionProps } from "./AttachmentAction.types";

export const AttachmentAction = (props: AttachmentActionProps) => {
  const merged = mergeProps({ variant: "ghost", size: "xs" } as const, props);

  return <Button {...merged} data-slot="attachment-action" />;
};
