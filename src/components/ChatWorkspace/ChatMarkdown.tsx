import { createMemo } from "solid-js";
import { toast } from "../../ui";
import { renderMarkdown } from "../../lib/markdown";
import { openExternal } from "../../api/external";

/**
 * A message body rendered as Markdown.
 *
 * The click handler exists because the app is a WebView: letting an anchor
 * navigate would replace the interface with whatever the other side sent. Every
 * link leaves through the system browser instead, and a scheme the backend
 * refuses never leaves the page at all.
 */
export function ChatMarkdown(props: { text: string }) {
  const html = createMemo(() => renderMarkdown(props.text));

  function handleClick(event: MouseEvent) {
    const anchor = (event.target as HTMLElement | null)?.closest("a");
    if (!anchor) return;
    event.preventDefault();
    const href = anchor.getAttribute("href");
    if (!href) return;
    openExternal(href).catch((cause: unknown) => {
      toast.error(cause instanceof Error ? cause.message : String(cause));
    });
  }

  return (
    // biome-ignore lint/security/noDangerouslySetInnerHtml: the HTML comes from
    // renderMarkdown, which sanitises it against a fixed tag and attribute list.
    <div class="chat-markdown" onClick={handleClick} innerHTML={html()} />
  );
}
