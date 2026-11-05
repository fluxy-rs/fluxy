import { mergeProps, splitProps } from "solid-js";
import classes from "./Card.styles";
import { clsx } from "../../utils";
import type { CardProps } from "./Card.types";

export const Card = (props: CardProps) => {
  const merged = mergeProps({ size: "md" } as const, props);
  const [local, others] = splitProps(merged, ["size", "class", "classList"]);

  return (
    <div
      data-slot="card"
      data-size={local.size}
      class={clsx(classes, local.class)}
      classList={local.classList}
      {...others}
    />
  );
};
