import { splitProps, type ValidComponent } from "solid-js";
import { Dynamic } from "solid-js/web";
import { clsx } from "../../utils";
import { bubbleContentClasses } from "./BubbleContent.styles";
import type { BubbleContentProps } from "./BubbleContent.types";

/**
 */
export const BubbleContent = <T extends ValidComponent = "div">(props: BubbleContentProps<T>) => {
  const [local, others] = splitProps(props as BubbleContentProps, [
    "component",
    "class",
    "classList",
  ]);

  return (
    <Dynamic
      {...others}
      component={(local.component as ValidComponent) ?? "div"}
      data-slot="bubble-content"
      class={clsx(bubbleContentClasses, local.class)}
      classList={local.classList}
    />
  );
};
