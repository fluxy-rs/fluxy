import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid({ hot: false })],
  resolve: {
    conditions: ["development", "browser"],
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    clearMocks: true,
    restoreMocks: true,
    server: {
      deps: {
        inline: ["lucide-solid"],
      },
    },
  },
});
