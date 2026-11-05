import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { AvatarBadgeProps } from "./AvatarBadge.types";

export const AvatarBadge = (props: AvatarBadgeProps) => {
  const [local, others] = splitProps(props, ["class", "classList"]);

  return (
    <span
      data-slot="avatar-badge"
      class={clsx(
        "absolute right-0 bottom-0 z-10 inline-flex items-center justify-center rounded-full",
        "bg-neutral-900 dark:bg-neutral-200 text-neutral-50 dark:text-neutral-900 bg-blend-color ring-2 ring-white dark:ring-neutral-950 select-none",
        "group-data-[size=sm]/avatar:size-2 group-data-[size=sm]/avatar:[&>svg]:hidden",
        "group-data-[size=md]/avatar:size-2.5 group-data-[size=md]/avatar:[&>svg]:size-2",
        "group-data-[size=lg]/avatar:size-3 group-data-[size=lg]/avatar:[&>svg]:size-2",
        local.class,
      )}
      classList={local.classList}
      {...others}
    />
  );
};
