import { describe, expect, it } from "vitest";
import { renderMarkdown } from "./markdown";

describe("renderMarkdown", () => {
  it("renders the emphasis, code and lists people type", () => {
    const html = renderMarkdown("**bold** and `code`\n\n- one\n- two");
    expect(html).toContain("<strong>bold</strong>");
    expect(html).toContain("<code>code</code>");
    expect(html).toContain("<li>one</li>");
  });

  it("keeps a single newline as a line break", () => {
    // Chat messages are typed with Enter, not with blank lines.
    expect(renderMarkdown("first\nsecond")).toContain("<br>");
  });

  it("turns a bare url into a link", () => {
    const html = renderMarkdown("see https://example.com/x");
    expect(html).toContain('href="https://example.com/x"');
  });

  it("drops raw html the sender embedded", () => {
    const html = renderMarkdown('hello <script>alert(1)</script><b onclick="x">b</b>');
    expect(html).not.toContain("<script");
    expect(html).not.toContain("onclick");
    // An unknown tag goes; its text stays, because the words are the message.
  });

  it("refuses a javascript url", () => {
    const html = renderMarkdown("[click](javascript:alert(1))");
    expect(html).not.toContain("javascript:");
  });

  it("drops images, which would be a remote request from the sender", () => {
    const html = renderMarkdown("![pixel](https://tracker.example/p.png)");
    expect(html).not.toContain("<img");
    expect(html).not.toContain("tracker.example");
  });

  it("renders a table", () => {
    const html = renderMarkdown("| a | b |\n|---|---|\n| 1 | 2 |");
    expect(html).toContain("<table>");
    expect(html).toContain("<td>1</td>");
  });
});
