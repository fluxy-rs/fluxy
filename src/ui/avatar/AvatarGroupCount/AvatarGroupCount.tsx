import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { AvatarGroupCountProps } from "./AvatarGroupCount.types";

export const AvatarGroupCount = (props: AvatarGroupCountProps) => {
  const [local, others] = splitProps(props, ["class", "classList"]);

  return (
    <div
      data-slot="avatar-group-count"
      class={clsx(
        "relative flex size-8 shrink-0 items-center justify-center rounded-full bg-neutral-100 dark:bg-neutral-800 text-sm text-neutral-500 dark:text-neutral-400 ring-2 ring-white dark:ring-neutral-950 group-has-data-[size=lg]/avatar-group:size-10 group-has-data-[size=sm]/avatar-group:size-6 [&>svg]:size-4 group-has-data-[size=lg]/avatar-group:[&>svg]:size-5 group-has-data-[size=sm]/avatar-group:[&>svg]:size-3",
        local.class,
      )}
      classList={local.classList}
      {...others}
    />
  );
};
