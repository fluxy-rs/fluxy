import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import classes from "./CardAction.styles";
import type { CardActionProps } from "./CardAction.types";

export const CardAction = (props: CardActionProps) => {
  const [local, rest] = splitProps(props, ["class", "classList"]);

  return (
    <div
      data-slot="card-action"
      class={clsx(classes, local.class)}
      classList={local.classList}
      {...rest}
    />
  );
};
