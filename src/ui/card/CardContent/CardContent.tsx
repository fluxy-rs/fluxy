import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import classes from "./CardContent.styles";
import type { CardContentProps } from "./CardContent.types";

export const CardContent = (props: CardContentProps) => {
  const [local, rest] = splitProps(props, ["class", "classList"]);

  return (
    <div
      data-slot="card-content"
      class={clsx(classes, local.class)}
      classList={local.classList}
      {...rest}
    />
  );
};
