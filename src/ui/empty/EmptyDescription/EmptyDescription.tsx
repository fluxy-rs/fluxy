import { splitProps } from "solid-js";
import type { ComponentProps } from "solid-js";
import { clsx } from "../../utils";

export function EmptyDescription(props: ComponentProps<"div">) {
  const [local, rest] = splitProps(props, ["class"]);
  return (
    <div
      data-slot="empty-description"
      class={clsx(
        "text-sm/relaxed text-muted-foreground [&>a]:underline [&>a]:underline-offset-4 [&>a:hover]:text-primary",
        local.class,
      )}
      {...rest}
    />
  );
}
