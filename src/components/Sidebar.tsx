import { Show, createSignal } from "solid-js";
import { A, useLocation } from "@solidjs/router";
import { copyText } from "../lib/format";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { PairingQrDialog } from "./PairingQrDialog";
import { QrCode } from "lucide-solid";
import { toast } from "../ui";
import { isActivePath, primaryNav } from "../nav";

/**
 * A 64px strip of icons, not a panel.
 *
 * It used to be 264px and carried the brand name, this device's name and id, a
 * QR button, a trust link and three labelled destinations - a column of mostly
 * low-frequency information sitting on top of the content for the whole
 * session. Everything that is not a destination moved to Settings, which is
 * where you go when you want to change something.
 */
export function Sidebar() {
  const engine = useEngine();
  const { t } = useI18n();
  const location = useLocation();
  const [qrOpen, setQrOpen] = createSignal(false);
  const [copied, setCopied] = createSignal(false);

  const status = () => engine.snapshot()?.status;

  async function copyDeviceId() {
    const id = status()?.device_id;
    if (!id) return;
    if (await copyText(id)) {
      setCopied(true);
      toast.success(t("common.copied"));
      window.setTimeout(() => setCopied(false), 1600);
    } else {
      toast.error(t("common.copyFailed"));
    }
  }

  // The dot reflects the same three states the label does, so the two cannot
  // disagree.
  const statusDotClass = () =>
    engine.loading()
      ? "bg-[var(--warning)]"
      : engine.snapshot()
        ? "bg-[var(--success)]"
        : "bg-[var(--destructive)]";

  return (
    <aside class="hidden h-full w-16 shrink-0 flex-col items-center gap-3 bg-sidebar py-3 text-sidebar-foreground md:flex">
      <div class="grid size-10 place-items-center" title="Fluxy">
        {/* The launcher icon itself, gradients and all. It carries its own
            rounded background, so the slot around it stays bare. */}
        <svg viewBox="0 0 128 128" role="img" aria-label="Fluxy">
          <defs>
            <linearGradient
              id="fluxy-brand-bg"
              x1="10"
              y1="6"
              x2="120"
              y2="124"
              gradientUnits="userSpaceOnUse"
            >
              <stop stop-color="#1B324B" />
              <stop offset=".55" stop-color="#102137" />
              <stop offset="1" stop-color="#0C1529" />
            </linearGradient>
            <linearGradient
              id="fluxy-brand-flow"
              x1="20"
              y1="41"
              x2="108"
              y2="87"
              gradientUnits="userSpaceOnUse"
            >
              <stop stop-color="#70F4C5" />
              <stop offset=".45" stop-color="#4ADBDD" />
              <stop offset="1" stop-color="#688CFC" />
            </linearGradient>
          </defs>
          <rect x="2" y="2" width="124" height="124" rx="28" fill="url(#fluxy-brand-bg)" />
          <rect
            x="3"
            y="3"
            width="122"
            height="122"
            rx="27"
            fill="none"
            stroke="#CBF8F8"
            stroke-opacity=".1"
            stroke-width="1.5"
          />
          <path
            d="M64 64 C53 45 45 42 37 42 C25 42 20 52 20 63 C20 75 26 85 38 85 C48 85 56 75 64 64 C72 53 80 43 90 43 C103 43 108 53 108 64 C108 76 102 85 90 85 C80 85 72 75 64 64 Z"
            fill="none"
            stroke="url(#fluxy-brand-flow)"
            stroke-width="10.5"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
      </div>

      <nav class="flex flex-col items-center gap-1" aria-label={t("nav.primary")}>
        {primaryNav.map((item) => {
          const Icon = item.icon;
          const active = () => isActivePath(location.pathname, item.href);
          return (
            <A
              href={item.href}
              class="grid size-10 place-items-center rounded-lg text-[var(--sidebar-foreground)] transition-colors hover:bg-white/10 hover:text-[var(--sidebar-foreground)] aria-[current=page]:bg-primary aria-[current=page]:text-primary-foreground"
              classList={{ "is-active": active() }}
              aria-current={active() ? "page" : undefined}
              aria-label={t(item.key)}
              title={t(item.key)}
            >
              <Icon size={20} aria-hidden="true" />
            </A>
          );
        })}
      </nav>

      <div class="mt-auto flex flex-col items-center gap-1">
        <button
          type="button"
          class="grid size-9 place-items-center rounded-lg text-[var(--sidebar-foreground)] transition-colors hover:bg-white/10 hover:text-[var(--sidebar-foreground)]"
          aria-label={t("pairing.myQrTitle")}
          title={t("pairing.myQrTitle")}
          onClick={() => setQrOpen(true)}
        >
          <QrCode size={19} aria-hidden="true" />
        </button>
        <button
          type="button"
          class="grid size-9 place-items-center rounded-lg text-[var(--sidebar-foreground)] transition-colors hover:bg-white/10 hover:text-[var(--sidebar-foreground)]"
          aria-label={t("home.copyId")}
          title={`${status()?.device_name ?? ""} · ${
            copied() ? t("common.copied") : (status()?.device_id ?? "")
          }`}
          onClick={() => void copyDeviceId()}
        >
          <span
            class="grid size-9 place-items-center rounded-full bg-white/10 text-xs font-semibold"
            aria-hidden="true"
          >
            {(status()?.device_name ?? "?").slice(0, 1).toUpperCase()}
          </span>
        </button>
      </div>

      <div class="mt-2 flex flex-col items-center gap-1 px-1">
        <span class={`size-1.5 rounded-full ${statusDotClass()}`} aria-hidden="true" />
        <span class="text-meta leading-none text-[var(--sidebar-foreground)]">
          {engine.loading()
            ? t("status.connecting")
            : engine.snapshot()
              ? t("status.ready")
              : t("status.offline")}
        </span>
      </div>

      <Show when={qrOpen()}>
        <PairingQrDialog open={qrOpen()} onOpenChange={setQrOpen} />
      </Show>
    </aside>
  );
}
