import { Show } from "solid-js";
import { CircleAlert, Copy, RefreshCw } from "lucide-solid";
import { useI18n } from "../i18n";
import { Button, toast } from "../ui";
import { copyText, errorDetails } from "../lib/format";
import type { EngineFailure } from "../state/engine";

export function ErrorState(props: {
  failure: EngineFailure | null;
  desktop: boolean;
  onRetry: () => void;
}) {
  const { t } = useI18n();
  const desktopFailure = () =>
    !props.desktop || props.failure?.code === "desktop_runtime";
  const details = () =>
    errorDetails({
      code: props.failure?.code ?? "unknown",
      message: props.failure?.message ?? t("error.unknown"),
      detail: props.failure?.detail,
      retryable: props.failure?.retryable,
    });

  async function copyDetails() {
    if (await copyText(details())) {
      toast.success(t("error.detailsCopied"));
    } else {
      toast.error(t("common.copyFailed"));
    }
  }

  return (
    <section class="fx-error" role="alert">
      <CircleAlert size={22} aria-hidden="true" />
      <div class="fx-error-copy">
        <h2>{desktopFailure() ? t("error.desktopTitle") : t("error.title")}</h2>
        <p>{desktopFailure() ? t("error.desktopBody") : t("error.body")}</p>
        <Show when={props.failure && !desktopFailure()}>
          <dl class="fx-error-meta">
            <div>
              <dt>{t("error.code")}</dt>
              <dd>{props.failure?.code ?? t("error.unknown")}</dd>
            </div>
            <div>
              <dt>{t("error.message")}</dt>
              <dd>{props.failure?.message ?? t("error.unknown")}</dd>
            </div>
          </dl>
          <details class="fx-error-details">
            <summary>{t("error.details")}</summary>
            <pre>{details()}</pre>
          </details>
        </Show>
        <div class="fx-error-actions">
          <Button onClick={props.onRetry} icon={<RefreshCw size={14} />}>
            {t("error.retry")}
          </Button>
          <Button
            variant="outline"
            onClick={() => void copyDetails()}
            icon={<Copy size={14} />}
          >
            {t("error.copyDetails")}
          </Button>
        </div>
      </div>
    </section>
  );
}

export function ErrorBanner(props: {
  failure: EngineFailure | null;
  onRetry: () => void;
}) {
  const { t } = useI18n();
  return (
    <div class="fx-error-banner" role="alert">
      <CircleAlert size={16} aria-hidden="true" />
      <span>{props.failure?.message ?? t("error.unknown")}</span>
      <Button variant="ghost" size="xs" onClick={props.onRetry}>
        {t("common.retry")}
      </Button>
    </div>
  );
}
