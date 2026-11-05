import {
  Tabs as NeutTabs,
  TabsContent as NeutTabsContent,
  TabsList as NeutTabsList,
  TabsTrigger as NeutTabsTrigger,
  type TabsContentProps,
  type TabsListProps,
  type TabsProps,
  type TabsTriggerProps,
} from "../tabs";
import { splitProps } from "solid-js";

export function Tabs(props: TabsProps) {
  const [local, rest] = splitProps(props, ["class"]);
  return <NeutTabs {...rest} class={local.class} />;
}

export function TabsList(props: TabsListProps) {
  const [local, rest] = splitProps(props, ["class", "variant"]);
  return <NeutTabsList {...rest} variant={local.variant ?? "line"} class={local.class} />;
}

export function TabsTrigger(props: TabsTriggerProps) {
  const [local, rest] = splitProps(props, ["class"]);
  return <NeutTabsTrigger {...rest} class={local.class} />;
}

export function TabsContent(props: TabsContentProps) {
  const [local, rest] = splitProps(props, ["class"]);
  return <NeutTabsContent {...rest} class={local.class} />;
}
