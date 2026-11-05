import { clsx } from "../../utils";

export const classes = clsx(
  "peer relative flex size-4 shrink-0 items-center justify-center rounded-[4px] border transition-colors outline-none group-has-disabled/field:opacity-50 after:absolute after:-inset-x-3 after:-inset-y-2 focus-visible:ring-3 data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-50 aria-invalid:ring-3",
  "border-neutral-200 focus-visible:border-neutral-400 focus-visible:ring-neutral-400/50 aria-invalid:border-red-600 aria-invalid:ring-red-600/20 aria-invalid:aria-checked:border-neutral-900 data-[checked=true]:border-neutral-900 data-[checked=true]:bg-neutral-900 data-[checked=true]:text-neutral-50",
  "dark:border-white/15 dark:focus-visible:border-neutral-500 dark:focus-visible:ring-neutral-500/50 dark:bg-input/30 dark:aria-invalid:border-red-400/50 dark:aria-invalid:ring-red-400/40 dark:data-[checked=true]:bg-neutral-200 dark:aria-invalid:aria-checked:border-neutral-200 dark:data-[checked=true]:border-neutral-200 dark:data-[checked=true]:text-neutral-900",
);
