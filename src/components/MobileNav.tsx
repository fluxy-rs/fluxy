import { For } from "solid-js";
import { useLocation, useNavigate } from "@solidjs/router";
import { useI18n } from "../i18n";
import { isActivePath, mobileNav } from "../nav";

export function MobileNav() {
  const { t } = useI18n();
  const location = useLocation();
  const navigate = useNavigate();
  return (
    <nav
      class="flex shrink-0 items-stretch justify-around border-t border-border bg-card pb-[env(safe-area-inset-bottom)] md:hidden"
      aria-label={t("nav.primary")}
    >
      <For each={mobileNav}>
        {(item) => {
          const Icon = item.icon;
          const active = () => isActivePath(location.pathname, item.href);
          return (
            <button
              type="button"
              class="flex flex-1 flex-col items-center gap-1 px-3 py-2 text-label text-muted-foreground aria-[current=page]:text-foreground"
              classList={{ "is-active": active() }}
              aria-current={active() ? "page" : undefined}
              // A real button with an explicit navigate: the anchor form relied on
              // the router rewriting href, and inside the Android WebView a tap on
              // it did nothing at all.
              onClick={() => navigate(item.href)}
            >
              <Icon size={20} aria-hidden="true" />
              <span>{t(item.key)}</span>
            </button>
          );
        }}
      </For>
    </nav>
  );
}
