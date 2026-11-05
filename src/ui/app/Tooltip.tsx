import { Tooltip as NeutTooltip, TooltipContent, TooltipTrigger } from "../tooltip";
import type { JSX } from "solid-js";
import { Button } from "./Button";

export function Tooltip(props: { label: string; children: JSX.Element }) {
  return (
    <NeutTooltip>
      <TooltipTrigger component={Button} variant="ghost" size="sm" class="size-8 p-0">
        {props.children}
      </TooltipTrigger>
      <TooltipContent class="text-xs">{props.label}</TooltipContent>
    </NeutTooltip>
  );
}

export function IconButton(props: {
  label: string;
  children: JSX.Element;
  onClick?: () => void;
  disabled?: boolean;
  active?: boolean;
}) {
  return (
    <NeutTooltip>
      <TooltipTrigger
        component={Button}
        variant="ghost"
        size="sm"
        class="size-8 p-0"
        classList={{ "bg-muted": props.active }}
        aria-label={props.label}
        onClick={props.onClick}
        disabled={props.disabled}
      >
        {props.children}
      </TooltipTrigger>
      <TooltipContent class="text-xs">{props.label}</TooltipContent>
    </NeutTooltip>
  );
}
