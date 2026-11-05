import { Progress as NeutProgress } from "../progress";

export function Progress(props: { value: number; label: string }) {
  const value = () => Math.max(0, Math.min(100, props.value));
  return (
    <div class="fx-progress" aria-live="polite">
      <div class="fx-progress-meta">
        <span>{props.label}</span>
        <span class="fx-numeric">{value()}%</span>
      </div>
      <NeutProgress value={value()} aria-label={props.label} class="fx-progress-track" />
    </div>
  );
}
