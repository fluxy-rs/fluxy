import type { Component } from "solid-js";
import { Clipboard, MessageCircle, Settings, Users } from "lucide-solid";

export type NavIcon = Component<{ size?: number; class?: string }>;

export type NavItem = {
  href: string;
  key: string;
  icon: NavIcon;
};

/**
 * Desktop keeps two side-by-side sections, the way a contacts app does:
 * "devices" is the address book - a list and, for the selected entry, its
 * details - while "messages" is the conversation list and the open thread.
 * A device's details answer "what is this machine", a thread answers "what did
 * we say"; merging them made a paired device appear twice.
 */
export const primaryNav: NavItem[] = [
  { href: "/devices", key: "nav.devices", icon: Users },
  { href: "/chat", key: "nav.chat", icon: MessageCircle },
  { href: "/clipboard", key: "nav.clipboard", icon: Clipboard },
  { href: "/settings", key: "nav.settings", icon: Settings },
];

/**
 * A phone shows one column at a time, so the same two sections are separate
 * destinations there.
 */
export const mobileNav: NavItem[] = [
  { href: "/devices", key: "nav.devices", icon: Users },
  { href: "/chat", key: "nav.chat", icon: MessageCircle },
  { href: "/clipboard", key: "nav.clipboard", icon: Clipboard },
  { href: "/settings", key: "nav.settings", icon: Settings },
];

export function isActivePath(current: string, href: string): boolean {
  // The device section owns both the list and a selected device's details, so
  // /devices matches itself and everything below it.
  return current === href || current.startsWith(href + "/");
}
