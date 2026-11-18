import DOMPurify from "dompurify";
import { marked } from "marked";

/**
 * Render a chat message as Markdown.
 *
 * `breaks` is on because people type a line break and mean it: without it every
 * newline inside a paragraph would collapse into a space. GFM is on so bare
 * links, tables and strikethrough work the way they do in every chat app.
 *
 * The text comes from another device, so the result is sanitised before it
 * reaches innerHTML. Raw HTML in the message is not a feature here - only the
 * tags Markdown itself produces survive - and DOMPurify refuses javascript:
 * URLs on the way through. Images are deliberately not allowed: a remote image
 * is a request the sender gets to see.
 */
const ALLOWED_TAGS = [
  "p",
  "br",
  "strong",
  "em",
  "del",
  "code",
  "pre",
  "blockquote",
  "ul",
  "ol",
  "li",
  "a",
  "h1",
  "h2",
  "h3",
  "h4",
  "h5",
  "h6",
  "hr",
  "table",
  "thead",
  "tbody",
  "tr",
  "th",
  "td",
];

const ALLOWED_ATTR = ["href", "title", "target", "rel"];

export function renderMarkdown(text: string): string {
  const html = marked.parse(text, { gfm: true, breaks: true, async: false }) as string;
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS,
    ALLOWED_ATTR,
    ALLOW_DATA_ATTR: false,
  });
}
