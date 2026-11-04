import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";

import { Switch } from "./Switch";

function surface(): HTMLElement {
  return document.querySelector('[data-slot="switch"]') as HTMLElement;
}

/**
 * The library's own tests cover clicks. These cover the keyboard, which the
 * library does not handle - see docs/neut-ui-gaps.local.md.
 */
describe("Switch keyboard", () => {
  it("activates with Space", () => {
    const onCheckedChange = vi.fn();
    render(() => <Switch onCheckedChange={onCheckedChange} />);

    fireEvent.keyDown(surface(), { key: " " });

    expect(onCheckedChange).toHaveBeenCalledWith(true);
    expect(surface()).toHaveAttribute("aria-checked", "true");
  });

  it("activates with Enter", () => {
    const onCheckedChange = vi.fn();
    render(() => <Switch onCheckedChange={onCheckedChange} />);

    fireEvent.keyDown(surface(), { key: "Enter" });

    expect(onCheckedChange).toHaveBeenCalledWith(true);
  });

  it("ignores other keys", () => {
    const onCheckedChange = vi.fn();
    render(() => <Switch onCheckedChange={onCheckedChange} />);

    fireEvent.keyDown(surface(), { key: "a" });

    expect(onCheckedChange).not.toHaveBeenCalled();
  });

  it("does nothing while disabled", () => {
    const onCheckedChange = vi.fn();
    render(() => <Switch disabled onCheckedChange={onCheckedChange} />);

    fireEvent.keyDown(surface(), { key: " " });

    expect(onCheckedChange).not.toHaveBeenCalled();
  });
});
