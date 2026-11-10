import type { JSX } from "solid-js";

/**
 * The box a page's content sits in.
 *
 * The shell drops its own padding below `md` so the device list and the
 * conversation workspace can run edge to edge. Ordinary pages then have to
 * bring a gutter of their own, and two of them never did, so on a phone their
 * title and cards sat flush against the window. Holding the width cap, the
 * section gap and the mobile gutter here means a new page inherits all three
 * instead of having to remember them.
 */
export function PageFrame(props: {
  /** Width cap; settings is deliberately narrower than the other pages. */
  width?: string;
  children: JSX.Element;
}) {
  return (
    <div class={`mx-auto flex w-full flex-col gap-4 max-md:p-4 ${props.width ?? "max-w-[60rem]"}`}>
      {props.children}
    </div>
  );
}
