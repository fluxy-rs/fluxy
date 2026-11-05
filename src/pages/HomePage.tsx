import { QrCode, ShieldCheck, Wifi } from "lucide-solid";
import { createSignal } from "solid-js";
import { Button } from "../ui";
import { useEngine } from "../state/engine";
import { useI18n } from "../i18n";
import { NearbyDevices } from "../components/NearbyDevices";
import { TrustedDevices } from "../components/TrustedDevices";
import { PairingQrDialog } from "../components/PairingQrDialog";

/**
 * The one screen an ordinary user needs: who is here, and how to talk to them.
 *
 * Device ids, listen ports, the discovery mechanism, the LAN scope diagram and
 * the event-channel counters used to live here as three side panels. They are
 * diagnostic detail - the answer to "why is it not working", asked once and
 * rarely - so they moved to Settings and left the first screen to the two
 * things that matter: the conversation and the device list.
 */
export function HomePage() {
  const engine = useEngine();
  const { t } = useI18n();
  const [qrOpen, setQrOpen] = createSignal(false);
  const snapshot = () => engine.snapshot()!;
  const peers = () => snapshot().peers.filter((peer) => peer.trusted);
  const nearby = () => snapshot().peers.filter((peer) => !peer.trusted);

  return (
    <div class="mx-auto flex w-full max-w-[60rem] flex-col gap-4">
      <header class="flex items-end justify-between gap-4">
        <div>
          <h1 class="text-display font-[620] leading-[1.04] tracking-[-0.035em]">
            {t("home.title")}
          </h1>
          <p class="text-body-sm text-muted-foreground">{t("home.subtitle")}</p>
        </div>
        <div class="flex items-center gap-2">
          <span class="rounded-full border border-border px-2.5 py-1 text-xs text-muted-foreground">
            <ShieldCheck size={14} aria-hidden="true" />
            {t("home.trustedCount", { count: peers().length })}
          </span>
          <span class="rounded-full border border-border px-2.5 py-1 text-xs text-muted-foreground">
            <Wifi size={14} aria-hidden="true" />
            {t("home.nearbyCount", { count: nearby().length })}
          </span>
          <div class="flex justify-center p-4">
            <Button variant="outline" onClick={() => setQrOpen(true)} icon={<QrCode size={15} />}>
              {t("pairing.myQrTitle")}
            </Button>
          </div>
        </div>
      </header>
      <NearbyDevices />

      {/* A desktop cannot scan, but a phone needs something to scan. Without
          this the phone scanning feature has no target on the desktop side. */}
      <PairingQrDialog open={qrOpen()} onOpenChange={setQrOpen} />

      {/* Trust management lives with the device list, not in Settings: who can
          reach you is the same question as who is around. Collapsed by default
          so it does not compete with the conversation. */}
      <details class="rounded-lg border border-border bg-card">
        <summary>{t("devices.trustedTitle")}</summary>
        <TrustedDevices />
      </details>
    </div>
  );
}
