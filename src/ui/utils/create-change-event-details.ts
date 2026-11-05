/**
 *
 *
 *
 */
export interface ChangeEventDetails<R extends string = string> {
  reason: R;
  event: Event | undefined;
  trigger: Element | undefined;
  cancel: () => void;
  allowPropagation: () => void;
  readonly isCanceled: boolean;
  readonly isPropagationAllowed: boolean;
}

export function createChangeEventDetails<R extends string = string>(
  reason: R,
  event?: Event,
  trigger?: Element,
): ChangeEventDetails<R> {
  let canceled = false;
  let propagationAllowed = false;

  return {
    reason,
    event,
    trigger,
    cancel: () => {
      canceled = true;
    },
    allowPropagation: () => {
      propagationAllowed = true;
    },
    get isCanceled() {
      return canceled;
    },
    get isPropagationAllowed() {
      return propagationAllowed;
    },
  };
}
