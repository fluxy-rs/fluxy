import type { ChatMessageView, TransferSummary } from "../../bindings";

/** One line of a conversation: a message or a file, both timed. */
export type TimelineEntry =
  | { kind: "text"; at: number; message: ChatMessageView }
  | { kind: "file"; at: number; transfer: TransferSummary };

/** What a long press was aimed at. The two kinds share a menu, not its items. */
export type MenuTarget =
  | { kind: "file"; transfer: TransferSummary }
  | { kind: "text"; message: ChatMessageView };

/** Where the finger pressed, so the menu can float beside that message. */
export type MenuAnchor = { x: number; y: number };
