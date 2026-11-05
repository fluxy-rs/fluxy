import { Show, mergeProps } from "solid-js";
import { X } from "lucide-solid";
import { Button } from "../../button";
import { callEventHandler } from "../../utils";
import { useDialogContext } from "../../dialog";
import type { SheetCloseProps } from "../sheet.types";

/**
 */
export const SheetClose = (props: SheetCloseProps) => {
  const { setOpen } = useDialogContext();

  const merged = mergeProps({ variant: "ghost", size: "sm" } as const, props);

  const handleClick = (event: MouseEvent) => {
    setOpen(false);
    callEventHandler(merged.onClick, event);
  };

  return (
    <Show
      when={!merged.icon && !merged.children}
      fallback={<Button {...merged} data-slot="sheet-close" onClick={handleClick} />}
    >
      <Button
        {...merged}
        data-slot="sheet-close"
        icon={<X />}
        aria-label="Close"
        onClick={handleClick}
      />
    </Show>
  );
};
