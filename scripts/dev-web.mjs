import { spawn } from "node:child_process";
import { connect } from "node:net";

// The desktop and the Android dev run share one Vite instance. Tauri invokes
// beforeDevCommand for both, so starting a second one on 8563 would fail with
// "Port 8563 is already in use" - probe first and reuse what is listening.
const PORT = 8563;

function probeOnce(host) {
  return new Promise((resolve) => {
    const socket = connect({ port: PORT, host });
    socket.once("connect", () => {
      socket.destroy();
      resolve(true);
    });
    socket.once("error", () => {
      socket.destroy();
      resolve(false);
    });
  });
}

async function main() {
  // Vite binds the host name it was given, so a running server may answer on
  // IPv6 only; check both families before deciding nothing is there.
  const listening = (await probeOnce("127.0.0.1")) || (await probeOnce("::1"));
  if (listening) {
    console.log(`[dev-web] port ${PORT} is already serving; reusing it`);
    return;
  }
  const child = spawn("bun", ["run", "start"], {
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  child.on("exit", (code) => process.exit(code ?? 0));
}

main();
