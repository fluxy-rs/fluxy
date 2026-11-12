import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createLongPress } from "./long-press";

function pointer(type: string, x: number, y: number) {
  return { pointerType: "touch", clientX: x, clientY: y, type } as unknown as PointerEvent;
}

describe("createLongPress", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("fires after the delay when the finger stays put", () => {
    const fired = vi.fn();
    const press = createLongPress(fired, 400);

    press.start(pointer("pointerdown", 10, 10));
    vi.advanceTimersByTime(399);
    expect(fired).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(fired).toHaveBeenCalledTimes(1);
  });

  it("cancels when the finger moves far enough", () => {
    const fired = vi.fn();
    const press = createLongPress(fired, 400, 10);

    press.start(pointer("pointerdown", 10, 10));
    press.move(pointer("pointermove", 40, 40));
    vi.advanceTimersByTime(500);

    expect(fired).not.toHaveBeenCalled();
  });

  it("keeps going for a small wobble", () => {
    const fired = vi.fn();
    const press = createLongPress(fired, 400, 10);

    press.start(pointer("pointerdown", 10, 10));
    press.move(pointer("pointermove", 14, 13));
    vi.advanceTimersByTime(400);

    expect(fired).toHaveBeenCalledTimes(1);
  });

  it("leaves a mouse to the desktop menu", () => {
    const fired = vi.fn();
    const press = createLongPress(fired, 400);

    press.start({ pointerType: "mouse", clientX: 0, clientY: 0 } as PointerEvent);
    vi.advanceTimersByTime(500);

    expect(fired).not.toHaveBeenCalled();
  });
});
