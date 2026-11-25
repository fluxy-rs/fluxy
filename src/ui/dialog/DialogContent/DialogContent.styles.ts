export const classes =
  // Centred by the box model rather than by a transform. The enter animation
  // writes a transform of its own, and with fill-mode both it keeps that
  // transform after the animation ends - which overrode the -translate-x/y-1/2
  // that used to centre this surface. inset-0 + m-auto needs no transform at all.
  "fixed inset-0 z-50 m-auto grid h-fit max-h-[calc(100dvh-2rem)] w-full max-w-[calc(100%-2rem)] gap-4 overflow-y-auto overscroll-contain rounded-xl bg-white p-4 text-sm text-neutral-950 ring-1 ring-neutral-950/10 duration-100 outline-none dark:bg-neutral-900 dark:text-neutral-50 dark:ring-neutral-50/10 sm:max-w-sm data-[open=true]:animate-in data-[open=true]:fade-in-0 data-[open=true]:zoom-in-95 data-[open=false]:animate-out data-[open=false]:fade-out-0 data-[open=false]:zoom-out-95";
