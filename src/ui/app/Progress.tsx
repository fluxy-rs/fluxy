import { Progress as NeutProgress } from "../progress";

export function Progress(props: { value: number; label: string }) {
  const value = () => Math.max(0, Math.min(100, props.value));
  return (
    <div class="flex flex-col gap-1" aria-live="polite">
      <div class="flex items-center justify-between gap-2 text-[11px] text-muted-foreground">
        <span>{props.label}</span>
        <span class="font-mono text-[11px]">{value()}%</span>
      </div>
      <NeutProgress
        value={value()}
        aria-label={props.label}
        class="h-1.5 overflow-hidden rounded-full bg-muted"
      />
    </div>
  );
}
