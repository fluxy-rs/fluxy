import { createSignal, mergeProps, splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { AvatarProps } from "./Avatar.types";
import { AvatarContext } from "./Avatar.context";

export const Avatar = (props: AvatarProps) => {
  const merged = mergeProps({ size: "md" } as const, props);

  const [local, others] = splitProps(merged, ["size", "class", "classList", "children"]);

  const [imageLoadFailed, setImageLoadFailed] = createSignal(false);
  const [imagePresent, setImagePresent] = createSignal(false);

  return (
    <span
      data-slot="avatar"
      data-size={local.size}
      class={clsx(
        "group/avatar relative flex size-8 shrink-0 rounded-full select-none after:absolute after:inset-0 after:rounded-full after:border after:border-neutral-200 dark:after:border-white/10 after:mix-blend-darken data-[size=lg]:size-10 data-[size=sm]:size-6 dark:after:mix-blend-lighten",
        local.class,
      )}
      classList={local.classList}
      {...others}
    >
      <AvatarContext.Provider
        value={{
          imageLoadFailed,
          setImageLoadFailed,
          imagePresent,
          setImagePresent,
        }}
      >
        {local.children}
      </AvatarContext.Provider>
    </span>
  );
};
