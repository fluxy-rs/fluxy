import { splitProps } from "solid-js";
import { clsx } from "../utils";
import { badgeVariants } from "./Badge.styles";
import type { BadgeProps } from "./Badge.types";

export const Badge = (props: BadgeProps) => {
  const [local, others] = splitProps(props, ["variant", "class", "classList"]);

  return (
    <span
      data-slot="badge"
      class={clsx(badgeVariants({ variant: local.variant }), local.class)}
      classList={local.classList}
      {...others}
    />
  );
};
