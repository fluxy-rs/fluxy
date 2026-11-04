import { useI18n } from "../i18n";

export function LoadingState() {
  const { t } = useI18n();
  return (
    <section class="fx-loading" role="status" aria-live="polite">
      <div class="fx-skeleton fx-skeleton-title" />
      <div class="fx-skeleton fx-skeleton-line" />
      <div class="fx-skeleton fx-skeleton-line fx-skeleton-short" />
      <p class="fx-loading-copy">{t("loading.title")}</p>
      <p class="fx-muted">{t("loading.body")}</p>
    </section>
  );
}
