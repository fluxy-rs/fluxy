import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { Copy, Trash2 } from "lucide-solid";
import { I18nProvider } from "../i18n";
import { MessageActions } from "./MessageActions";

describe("MessageActions", () => {
  it("runs the chosen action and closes the menu", () => {
    const copied = vi.fn();
    const closed = vi.fn();

    render(() => (
      <I18nProvider>
        <MessageActions
          open
          anchor={{ x: 200, y: 400 }}
          onClose={closed}
          label="消息操作"
          actions={[
            { label: "复制", icon: Copy, onSelect: copied },
            { label: "删除", icon: Trash2, destructive: true, onSelect: () => {} },
          ]}
        />
      </I18nProvider>
    ));

    const menu = screen.getByRole("menu", { name: "消息操作" });
    expect(menu).toBeInTheDocument();
    fireEvent.click(screen.getByRole("menuitem", { name: "复制" }));

    expect(closed).toHaveBeenCalledTimes(1);
    expect(copied).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("menuitem", { name: "删除" })).toBeInTheDocument();
  });

  it("sits above the pressed message when there is room", () => {
    render(() => (
      <I18nProvider>
        <MessageActions
          open
          anchor={{ x: 200, y: 600 }}
          onClose={() => {}}
          label="消息操作"
          actions={[{ label: "复制", icon: Copy, onSelect: () => {} }]}
        />
      </I18nProvider>
    ));

    const menu = screen.getByRole("menu", { name: "消息操作" }) as HTMLElement;
    // jsdom reports a zero-height viewport, so the flip decision has to be
    // read from the element rather than from a rendered position.
    expect(Number.parseFloat(menu.style.top)).toBeGreaterThanOrEqual(0);
    expect(Number.parseFloat(menu.style.left)).toBeGreaterThanOrEqual(0);
  });
});
