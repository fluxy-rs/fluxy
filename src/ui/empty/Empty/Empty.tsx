import { splitProps } from "solid-js";
import type { ComponentProps } from "solid-js";
import { clsx } from "../../utils";

export function Empty(props: ComponentProps<"div">) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <div
      data-slot="empty"
      class={clsx(
        "flex w-full min-w-0 flex-1 flex-col items-center justify-center gap-4 rounded-xl border-dashed p-6 text-center text-balance",
        local.class,
      )}
      {...rest}
    />
  );
}
