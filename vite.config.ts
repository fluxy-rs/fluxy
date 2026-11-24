import tailwind from "@tailwindcss/vite";
import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
// @ts-expect-error type error without @types/node package
import process from "node:process";
import { execFileSync } from "node:child_process";
const host = process.env.TAURI_DEV_HOST;

/// Identifies the build that is actually running.
///
/// Every APK here carries the same semver, so "did the device get the new one"
/// could only be answered by guessing. This pairs the commit with the minute it
/// was compiled, and the environment card shows it.
function buildId(): string {
  const commit = (() => {
    try {
      return execFileSync("git", ["rev-parse", "--short", "HEAD"], { encoding: "utf8" }).trim();
    } catch {
      return "unknown";
    }
  })();
  const stamp = new Date().toISOString().slice(0, 16).replace("T", " ");
  return `${commit} @ ${stamp}`;
}

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [solid(), tailwind()],

  define: { __BUILD_ID__: JSON.stringify(buildId()) },

  // lucide-solid ships one file per icon. Without pre-bundling, a dev page that
  // imports a handful of them pulls in two thousand separate module requests
  // and the first paint never arrives.
  optimizeDeps: {
    include: ["lucide-solid"],
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 8563,
    strictPort: true,
    // Bind every interface: the phone loads this server over the LAN through
    // `devUrl`, and with the default the listener is localhost-only.
    host: host || "0.0.0.0",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 8564,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
