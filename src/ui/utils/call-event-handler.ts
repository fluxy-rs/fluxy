/**
 *
 *
 * - bound handler：`{ 0: (data, event) => void; 1: data }`——
 *
 */
export function callEventHandler<E extends Event>(handler: unknown, event: E): void {
  if (handler == null) return;

  const isBound =
    Array.isArray(handler) || (typeof handler === "object" && handler !== null && 0 in handler);

  if (isBound) {
    const slot = handler as Record<number, unknown>;
    const boundHandler = slot[0];
    const data = slot[1];
    if (typeof boundHandler === "function") {
      (boundHandler as (data: unknown, event: E) => void)(data, event);
    }
    return;
  }

  if (typeof handler === "function") (handler as (event: E) => void)(event);
}
