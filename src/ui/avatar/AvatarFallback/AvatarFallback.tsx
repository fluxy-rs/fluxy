import { Show, splitProps } from "solid-js";
import { clsx } from "../../utils";
import type { AvatarFallbackProps } from "./AvatarFallback.types";
import { useAvatarContext } from "../Avatar/Avatar.context";

export const AvatarFallback = (props: AvatarFallbackProps) => {
  const { imageLoadFailed, imagePresent } = useAvatarContext();

  const [local, others] = splitProps(props, ["class", "classList"]);

  return (
    <Show when={!imagePresent() || imageLoadFailed()}>
      <span
        data-slot="avatar-fallback"
        class={clsx(
          "flex size-full items-center justify-center rounded-full bg-neutral-100 dark:bg-neutral-800 text-sm text-neutral-500 dark:text-neutral-400 group-data-[size=sm]/avatar:text-xs",
          local.class,
        )}
        classList={local.classList}
        {...others}
      />
    </Show>
  );
};
