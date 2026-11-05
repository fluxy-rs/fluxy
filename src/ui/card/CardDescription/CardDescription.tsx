import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import classes from "./CardDescription.styles";
import type { CardDescriptionProps } from "./CardDescription.types";

export const CardDescription = (props: CardDescriptionProps) => {
  const [local, rest] = splitProps(props, ["class", "classList"]);

  return (
    <div
      data-slot="card-description"
      class={clsx(classes, local.class)}
      classList={local.classList}
      {...rest}
    />
  );
};
