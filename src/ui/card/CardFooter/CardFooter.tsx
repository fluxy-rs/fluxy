import { splitProps } from "solid-js";
import { clsx } from "../../utils";
import classes from "./CardFooter.styles";
import type { CardFooterProps } from "./CardFooter.types";

export const CardFooter = (props: CardFooterProps) => {
  const [local, rest] = splitProps(props, ["class", "classList"]);

  return (
    <div
      data-slot="card-footer"
      class={clsx(classes, local.class)}
      classList={local.classList}
      {...rest}
    />
  );
};
