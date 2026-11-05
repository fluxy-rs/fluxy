import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { AvatarGroupProps } from "./AvatarGroup.types";

export const AvatarGroup = (props: AvatarGroupProps) => {
  const [local, others] = splitProps(props, ["class", "classList"]);

  return (
    <div
      data-slot="avatar-group"
      class={clsx(
        "group/avatar-group flex -space-x-2 *:data-[slot=avatar]:ring-2 *:data-[slot=avatar]:ring-white dark:*:data-[slot=avatar]:ring-neutral-950",
        local.class,
      )}
      classList={local.classList}
      {...others}
    />
  );
};
