import { Textarea as NeutTextarea, type TextareaProps } from "../textarea";

/**
 * The chat composer's textarea.
 *
 * No application styling of its own: the library's field already carries the
 * border, focus ring, sizing and disabled states. This exists so the composer
 * imports from `../ui` like every other control instead of reaching into the
 * component tree.
 */
export function Textarea(props: TextareaProps) {
  return <NeutTextarea {...props} />;
}
