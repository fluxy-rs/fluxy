import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import classes from "./CardTitle.styles";
import type { CardTitleProps } from "./CardTitle.types";

export const CardTitle = (props: CardTitleProps) => {
  const [local, rest] = splitProps(props, ["class", "classList"]);

  return (
    <div
      data-slot="card-title"
      class={clsx(classes, local.class)}
      classList={local.classList}
      {...rest}
    />
  );
};
