/**
 * Class merging, re-exported from `cn`.
 *
 * `cn` is the library's own drop-in for `twMerge(clsx(...))` and exports both
 * names, so a component copied out of it keeps its `import { clsx } from
 * "../cn"` line unchanged. It replaces the clsx and tailwind-merge pair the
 * application used to carry.
 */
export { clsx, cn } from "cn";
