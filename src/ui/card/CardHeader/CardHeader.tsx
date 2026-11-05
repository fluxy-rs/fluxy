import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import classes from "./CardHeader.styles";
import type { CardHeaderProps } from "./CardHeader.types";

export const CardHeader = (props: CardHeaderProps) => {
  const [local, rest] = splitProps(props, ["class", "classList"]);

  return (
    <div
      data-slot="card-header"
      class={clsx(classes, local.class)}
      classList={local.classList}
      {...rest}
    />
  );
};
