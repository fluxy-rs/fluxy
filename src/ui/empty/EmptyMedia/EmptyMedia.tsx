import { splitProps } from "solid-js";
import type { ComponentProps } from "solid-js";
import { clsx } from "../../utils";
import { emptyMediaVariants } from "./EmptyMedia.styles";

export function EmptyMedia(
  props: ComponentProps<"div"> & {
    variant?: "default" | "icon";
  },
) {
  const [local, rest] = splitProps(props, ["variant", "class"]);
  return (
    <div
      data-slot="empty-icon"
      data-variant={local.variant}
      class={clsx(
        emptyMediaVariants({
          variant: local.variant ?? "default",
          class: local.class,
        }),
      )}
      {...rest}
    />
  );
}
