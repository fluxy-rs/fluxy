import { fireEvent, render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { Copy, Trash2 } from "lucide-solid";
import { I18nProvider } from "../i18n";
import { MessageActions } from "./MessageActions";

describe("MessageActions", () => {
  it("runs the chosen action and closes the sheet", () => {
    const copied = vi.fn();
    const closed = vi.fn();

    render(() => (
      <I18nProvider>
        <MessageActions
          open
          onClose={closed}
          label="消息操作"
          actions={[
            { label: "复制", icon: Copy, onSelect: copied },
            { label: "删除", icon: Trash2, destructive: true, onSelect: () => {} },
          ]}
        />
      </I18nProvider>
    ));

    fireEvent.click(screen.getByRole("button", { name: "复制" }));

    expect(closed).toHaveBeenCalledTimes(1);
    expect(copied).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "删除" })).toBeInTheDocument();
  });
});
