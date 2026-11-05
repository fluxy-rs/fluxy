import { splitProps } from "solid-js";
import type { ComponentProps } from "solid-js";
import { clsx } from "../../utils";

export function EmptyHeader(props: ComponentProps<"div">) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <div
      data-slot="empty-header"
      class={clsx("flex max-w-sm flex-col items-center gap-2", local.class)}
      {...rest}
    />
  );
}
