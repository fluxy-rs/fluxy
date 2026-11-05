import { Tooltip as NeutTooltip, TooltipContent, TooltipTrigger } from "../tooltip";
import type { JSX } from "solid-js";
import { Button } from "./Button";

export function Tooltip(props: { label: string; children: JSX.Element }) {
  return (
    <NeutTooltip>
      <TooltipTrigger component={Button} variant="ghost" size="sm" class="fx-tooltip-trigger">
        {props.children}
      </TooltipTrigger>
      <TooltipContent class="fx-tooltip">{props.label}</TooltipContent>
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
        class={`fx-icon-button ${props.active ? "is-active" : ""}`}
        aria-label={props.label}
        onClick={props.onClick}
        disabled={props.disabled}
      >
        {props.children}
      </TooltipTrigger>
      <TooltipContent class="fx-tooltip">{props.label}</TooltipContent>
    </NeutTooltip>
  );
}
