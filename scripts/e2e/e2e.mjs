#!/usr/bin/env node
// M1-09 end-to-end acceptance driver for the Fluxy CLI.
// Modes:
//   local            two processes on this host over loopback (default)
//   serve            start a LAN server for a peer on another host
//   join             pair/chat/clipboard against a remote LAN server
//
// This driver auto-confirms the six-digit SAS. It is a test harness, not a
// production tool: run it only on an isolated test LAN.

import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { networkInterfaces, tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(HERE, "..", "..");
const IS_WINDOWS = process.platform === "win32";
const DEFAULT_BIN = path.join(REPO_ROOT, "target", "debug", IS_WINDOWS ? "fluxy-cli.exe" : "fluxy-cli");
const DEFAULT_TIMEOUT_MS = 30000;

function parseArgs(argv) {
  const options = { _: [], yes: false, build: true, keep: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (!token.startsWith("--")) {
      options._.push(token);
      continue;
    }
    const body = token.slice(2);
    const equals = body.indexOf("=");
    if (equals >= 0) {
      options[body.slice(0, equals)] = body.slice(equals + 1);
      continue;
    }
    const next = argv[index + 1];
    if (next !== undefined && !next.startsWith("--")) {
      options[body] = next;
      index += 1;
      continue;
    }
    options[body] = true;
  }
  if (options["no-build"]) {
    options.build = false;
  }
  return options;
}

function log(message) {
  process.stdout.write("[e2e] " + message + "\n");
}

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function assert(condition, message, detail) {
  if (!condition) {
    throw new Error(detail ? message + "\n" + detail : message);
  }
}

class CliProcess {
  constructor(bin, args, options) {
    const settings = options || {};
    this.bin = bin;
    this.args = args;
    this.stdoutText = "";
    this.stderrText = "";
    this.exit = null;
    this.spawnError = null;
    this.child = spawn(bin, args, {
      cwd: settings.cwd || REPO_ROOT,
      env: Object.assign({}, process.env, settings.env || {}),
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });
    this.child.stdout.setEncoding("utf8");
    this.child.stderr.setEncoding("utf8");
    this.child.stdout.on("data", (chunk) => {
      this.stdoutText = (this.stdoutText + chunk).replace(/\r\n/g, "\n");
      if (settings.echo) process.stdout.write(chunk);
    });
    this.child.stderr.on("data", (chunk) => {
      this.stderrText = (this.stderrText + chunk).replace(/\r\n/g, "\n");
      if (settings.echo) process.stderr.write(chunk);
    });
    this.child.on("error", (error) => {
      this.spawnError = error;
    });
    this.child.on("exit", (code, signal) => {
      this.exit = { code, signal };
    });
  }

  write(text) {
    if (!this.child.stdin.destroyed) {
      this.child.stdin.write(text);
    }
  }

  writeLine(text) {
    this.write(text.endsWith("\n") ? text : text + "\n");
  }

  async waitFor(stream, regex, timeoutMs, label) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      if (this.spawnError) {
        throw new Error("failed to spawn " + this.bin + ": " + this.spawnError.message);
      }
      const text = (stream === "stderr" ? this.stderrText : this.stdoutText).replace(/\r/g, "");
      const match = regex.exec(text);
      if (match) {
        return match;
      }
      await delay(50);
    }
    throw new Error(
      "timed out waiting for " + label + "\nstdout:\n" + this.stdoutText + "\nstderr:\n" + this.stderrText,
    );
  }

  async waitForExit(timeoutMs) {
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      if (this.exit) {
        return this.exit;
      }
      if (this.spawnError) {
        throw new Error("failed to spawn " + this.bin + ": " + this.spawnError.message);
      }
      await delay(50);
    }
    await this.stop();
    throw new Error(
      "process timed out after " + timeoutMs + " ms: " + this.bin + " " + this.args.join(" ") +
        "\nstdout:\n" + this.stdoutText + "\nstderr:\n" + this.stderrText,
    );
  }

  async waitForever() {
    while (!this.exit) {
      await delay(200);
    }
  }

  async stop() {
    if (this.exit) {
      return;
    }
    try {
      this.child.kill();
    } catch (error) {
      // The process may have exited between the check and the kill.
    }
    const deadline = Date.now() + 5000;
    while (Date.now() < deadline) {
      if (this.exit) {
        return;
      }
      await delay(50);
    }
    try {
      this.child.kill("SIGKILL");
    } catch (error) {
      // Ignore a second kill failure.
    }
    const hardDeadline = Date.now() + 2000;
    while (Date.now() < hardDeadline) {
      if (this.exit) {
        return;
      }
      await delay(50);
    }
  }
}

async function runCli(bin, args, options) {
  const settings = options || {};
  const proc = new CliProcess(bin, args, settings);
  if (settings.input) {
    proc.write(settings.input);
  }
  const exit = await proc.waitForExit(settings.timeoutMs || DEFAULT_TIMEOUT_MS);
  return {
    code: exit.code,
    signal: exit.signal,
    stdout: proc.stdoutText.replace(/\r/g, ""),
    stderr: proc.stderrText.replace(/\r/g, ""),
  };
}

function buildCli() {
  log("building fluxy-cli");
  const result = spawnSync("cargo", ["build", "-q", "-p", "fluxy-cli"], {
    cwd: REPO_ROOT,
    stdio: "inherit",
    shell: IS_WINDOWS,
  });
  if (result.error) {
    throw new Error("could not run cargo: " + result.error.message);
  }
  if (result.status !== 0) {
    throw new Error("cargo build -p fluxy-cli failed with status " + result.status);
  }
}

function resolveBin(options) {
  if (options.bin) {
    return path.resolve(REPO_ROOT, options.bin);
  }
  return DEFAULT_BIN;
}

function listLanAddresses() {
  const addresses = [];
  const interfaces = networkInterfaces();
  for (const name of Object.keys(interfaces)) {
    for (const entry of interfaces[name] || []) {
      if (entry.family === "IPv4" && !entry.internal) {
        addresses.push(entry.address);
      }
    }
  }
  return addresses;
}

function autoConfirm(proc) {
  let confirmed = 0;
  const timer = setInterval(() => {
    const count = (proc.stderrText.match(/SAS:/g) || []).length;
    while (confirmed < count) {
      proc.writeLine("yes");
      confirmed += 1;
    }
  }, 50);
  if (typeof timer.unref === "function") {
    timer.unref();
  }
  return () => clearInterval(timer);
}

async function startServer(bin, dataDir) {
  const proc = new CliProcess(bin, ["--data-dir", dataDir, "serve", "--listen", "127.0.0.1:0", "--dev-loopback"]);
  const stopConfirm = autoConfirm(proc);
  const listen = (await proc.waitFor("stdout", /^listen=(\S+)$/m, 20000, "server listen address"))[1];
  const device = (await proc.waitFor("stdout", /^device=(\S+)$/m, 20000, "server device id"))[1];
  const fingerprint = (await proc.waitFor("stdout", /^fingerprint=([0-9a-f]{64})$/m, 20000, "server fingerprint"))[1];
  return { proc, stopConfirm, listen, device, fingerprint };
}

async function stopServer(handle) {
  handle.stopConfirm();
  await handle.proc.stop();
}

async function localScenario(bin, options) {
  const root = mkdtempSync(path.join(tmpdir(), "fluxy-e2e-"));
  const serverDir = path.join(root, "server");
  const clientDir = path.join(root, "client");
  const guardDir = path.join(root, "guard");
  const context = {};
  let passed = 0;

  const step = async (name, action) => {
    log("step: " + name);
    await action();
    passed += 1;
    log("ok: " + name);
  };

  try {
    await step("first pairing over loopback", async () => {
      const handle = await startServer(bin, serverDir);
      context.listen = handle.listen;
      context.device = handle.device;
      context.fingerprint = handle.fingerprint;
      try {
        const pair = await runCli(
          bin,
          ["--data-dir", clientDir, "pair", handle.listen, "--fingerprint", handle.fingerprint, "--dev-loopback"],
          { input: "yes\n" },
        );
        assert(pair.code === 0, "pairing exited with code " + pair.code, pair.stderr);
        assert(/^paired device=(\S+) name=.* sas=\d{6}$/m.test(pair.stdout), "pairing output missing SAS", pair.stdout);
        const pairedDevice = /^paired device=(\S+)/m.exec(pair.stdout)[1];
        assert(pairedDevice === handle.device, "client paired " + pairedDevice + " but server is " + handle.device);
        // The one-shot pair command closes right after writing trust, so the
        // server records the pairing and then reports a benign control-stream
        // close. A later server restart proves the persisted trust.
        await handle.proc.waitFor("stderr", /^SAS: \d{6}$/m, 20000, "server SAS prompt");
      } finally {
        await stopServer(handle);
      }
    });

    await step("trust persists across restart and chat delivers an ACK", async () => {
      const handle = await startServer(bin, serverDir);
      assert(handle.device === context.device, "device id changed across restart", handle.device);
      assert(handle.fingerprint === context.fingerprint, "certificate fingerprint changed across restart", handle.fingerprint);
      try {
        const message = "m1-09-hello-" + Date.now();
        const chat = await runCli(
          bin,
          ["--data-dir", clientDir, "chat", handle.listen, "--device", handle.device, "--message", message, "--dev-loopback"],
        );
        assert(chat.code === 0, "chat exited with code " + chat.code, chat.stderr);
        assert(/^resumed device=/m.test(chat.stdout), "chat did not resume the paired trust", chat.stdout);
        assert(/^ack message=\S+ delivered=true/m.test(chat.stdout), "chat did not receive a delivered ACK", chat.stdout);
        await handle.proc.waitFor("stdout", /^message: /m, 20000, "server chat message");
        assert(
          handle.proc.stdoutText.replace(/\r/g, "").includes("message: " + message + "\n"),
          "server did not receive the exact chat text",
          handle.proc.stdoutText,
        );
        assert(!/SAS:/.test(handle.proc.stderrText), "resumed server asked for SAS again", handle.proc.stderrText);
        assert(!/^paired device=/m.test(handle.proc.stdoutText), "resumed server performed a new pairing", handle.proc.stdoutText);
      } finally {
        await stopServer(handle);
      }
    });

    await step("clipboard consent gate", async () => {
      const handle = await startServer(bin, serverDir);
      try {
        const text = "m1-09-clipboard-" + Date.now();
        const clipboard = await runCli(
          bin,
          ["--data-dir", clientDir, "clipboard", handle.listen, "--device", handle.device, "--text", text, "--dev-loopback"],
        );
        assert(
          clipboard.code === 0,
          "clipboard exited with code " + clipboard.code,
          "client stderr:\n" + clipboard.stderr + "\nserver stdout:\n" + handle.proc.stdoutText + "\nserver stderr:\n" + handle.proc.stderrText,
        );
        assert(
          /^clipboard-rejected .*code=cli_no_consent/m.test(clipboard.stdout),
          "headless receiver did not reject clipboard consent",
          clipboard.stdout,
        );
        await handle.proc.waitFor("stdout", /^clipboard-offer /m, 20000, "server clipboard offer");
      } finally {
        await stopServer(handle);
      }
    });

    await step("untrusted device is rejected", async () => {
      const handle = await startServer(bin, serverDir);
      try {
        const stranger = "00000000-0000-7000-8000-000000000001";
        const chat = await runCli(
          bin,
          ["--data-dir", clientDir, "chat", handle.listen, "--device", stranger, "--message", "nope", "--dev-loopback"],
        );
        assert(chat.code !== 0, "untrusted chat unexpectedly succeeded", chat.stdout);
        assert(/not trusted|untrusted/i.test(chat.stderr), "untrusted chat error was not explicit", chat.stderr);
      } finally {
        await stopServer(handle);
      }
    });

    await step("loopback requires explicit dev mode", async () => {
      const guard = await runCli(bin, ["--data-dir", guardDir, "serve", "--listen", "127.0.0.1:0"], {
        timeoutMs: 20000,
      });
      assert(guard.code !== 0, "loopback serve without --dev-loopback unexpectedly succeeded", guard.stdout);
      assert(
        /loopback listener requires --dev-loopback/.test(guard.stderr),
        "loopback guard message missing",
        guard.stderr,
      );
    });
  } finally {
    if (options.keep) {
      log("kept temporary directory " + root);
    } else {
      rmSync(root, { recursive: true, force: true });
    }
  }
  log("local scenario passed " + passed + " steps");
  return passed;
}

async function serveScenario(bin, options) {
  if (!options.yes) {
    throw new Error("serve mode requires --yes: this harness auto-confirms the six-digit SAS");
  }
  const ephemeral = !options["data-dir"];
  const dataDir = options["data-dir"]
    ? path.resolve(REPO_ROOT, options["data-dir"])
    : mkdtempSync(path.join(tmpdir(), "fluxy-e2e-server-"));
  const listen = options.listen || "0.0.0.0:0";
  const devLoopback = listen.startsWith("127.") || listen.startsWith("localhost");
  const serveArgs = ["--data-dir", dataDir, "serve", "--listen", listen];
  if (devLoopback) {
    serveArgs.push("--dev-loopback");
  }
  const proc = new CliProcess(bin, serveArgs, { echo: true });
  const stopConfirm = autoConfirm(proc);
  const onSignal = async () => {
    stopConfirm();
    await proc.stop();
    process.exit(130);
  };
  process.once("SIGINT", onSignal);
  process.once("SIGTERM", onSignal);
  try {
    const listenAddr = (await proc.waitFor("stdout", /^listen=(\S+)$/m, 20000, "server listen address"))[1];
    const device = (await proc.waitFor("stdout", /^device=(\S+)$/m, 20000, "server device id"))[1];
    const fingerprint = (await proc.waitFor("stdout", /^fingerprint=([0-9a-f]{64})$/m, 20000, "server fingerprint"))[1];
    const port = listenAddr.slice(listenAddr.lastIndexOf(":") + 1);
    const connectHosts = devLoopback ? ["127.0.0.1"] : listLanAddresses();
    if (connectHosts.length === 0) {
      connectHosts.push("127.0.0.1");
    }
    const connectAddrs = connectHosts.map((host) => host + ":" + port);
    const connectAddr = connectAddrs[0];
    process.stdout.write("FLUXY_E2E_SERVER listen=" + listenAddr + "\n");
    for (const candidate of connectAddrs) {
      process.stdout.write("FLUXY_E2E_SERVER connect=" + candidate + "\n");
    }
    process.stdout.write("FLUXY_E2E_SERVER device=" + device + "\n");
    process.stdout.write("FLUXY_E2E_SERVER fingerprint=" + fingerprint + "\n");
    log("run on the other host:");
    log("  scripts/e2e/run-e2e.sh join --addr " + connectAddr + " --fingerprint " + fingerprint + " --device " + device + " --yes");
    log("server running; press Ctrl+C to stop");
    await proc.waitForever();
    if (proc.exit && proc.exit.code !== 0) {
      throw new Error("server exited with code " + proc.exit.code);
    }
  } finally {
    stopConfirm();
    process.removeListener("SIGINT", onSignal);
    process.removeListener("SIGTERM", onSignal);
    await proc.stop();
    if (ephemeral && !options.keep) {
      rmSync(dataDir, { recursive: true, force: true });
    } else if (ephemeral) {
      log("kept temporary directory " + dataDir);
    }
  }
}

async function joinScenario(bin, options) {
  if (!options.yes) {
    throw new Error("join mode requires --yes: this harness auto-confirms the six-digit SAS");
  }
  const addr = options.addr;
  const fingerprint = options.fingerprint;
  const device = options.device;
  if (!addr || !fingerprint || !device) {
    throw new Error("join mode requires --addr, --fingerprint, and --device");
  }
  const root = mkdtempSync(path.join(tmpdir(), "fluxy-e2e-join-"));
  const dataDir = options["data-dir"] ? path.resolve(REPO_ROOT, options["data-dir"]) : path.join(root, "client");
  const devArgs = options["dev-loopback"] ? ["--dev-loopback"] : [];
  try {
    const pair = await runCli(bin, ["--data-dir", dataDir, "pair", addr, "--fingerprint", fingerprint, ...devArgs], {
      input: "yes\n",
    });
    assert(pair.code === 0, "remote pairing exited with code " + pair.code, pair.stderr);
    assert(/^paired device=/m.test(pair.stdout), "remote pairing output missing device", pair.stdout);

    const message = options.message || "m1-09-remote-" + Date.now();
    const chat = await runCli(bin, ["--data-dir", dataDir, "chat", addr, "--device", device, "--message", message, ...devArgs]);
    assert(chat.code === 0, "remote chat exited with code " + chat.code, chat.stderr);
    assert(/^ack message=\S+ delivered=true/m.test(chat.stdout), "remote chat did not receive a delivered ACK", chat.stdout);

    const text = options.text || "m1-09-remote-clipboard-" + Date.now();
    const clipboard = await runCli(bin, ["--data-dir", dataDir, "clipboard", addr, "--device", device, "--text", text, ...devArgs]);
    assert(clipboard.code === 0, "remote clipboard exited with code " + clipboard.code, clipboard.stderr);
    assert(
      /^clipboard-(rejected|applied)/m.test(clipboard.stdout),
      "remote clipboard did not reach a terminal state",
      clipboard.stdout,
    );
    log("remote scenario passed");
  } finally {
    if (options.keep) {
      log("kept temporary directory " + root);
    } else {
      rmSync(root, { recursive: true, force: true });
    }
  }
}

function usage() {
  process.stdout.write("usage: node scripts/e2e/e2e.mjs [local|serve|join] [options]\n");
  process.stdout.write("\n");
  process.stdout.write("  local                 pair, chat, clipboard, restart, and reject on this host (default)\n");
  process.stdout.write("  serve                 run a LAN server for a peer on another host (requires --yes)\n");
  process.stdout.write("  join                  run against a remote LAN server (requires --addr/--fingerprint/--device/--yes)\n");
  process.stdout.write("\n");
  process.stdout.write("options:\n");
  process.stdout.write("  --bin PATH            fluxy-cli binary (default target/debug/fluxy-cli)\n");
  process.stdout.write("  --no-build            skip cargo build -p fluxy-cli\n");
  process.stdout.write("  --keep                keep temporary data directories\n");
  process.stdout.write("  --data-dir PATH       serve/join data directory override\n");
  process.stdout.write("  --listen ADDR         serve listen address (default 0.0.0.0:0)\n");
  process.stdout.write("  --addr ADDR           join peer address\n");
  process.stdout.write("  --fingerprint HEX     join peer certificate fingerprint\n");
  process.stdout.write("  --device UUID         join peer device id\n");
  process.stdout.write("  --dev-loopback        allow loopback addresses (local cross-host rehearsal)\n");
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const mode = options._[0] || "local";
  if (options.help) {
    usage();
    return;
  }
  if (options.build) {
    buildCli();
  }
  const bin = resolveBin(options);
  if (!existsSync(bin)) {
    throw new Error("fluxy-cli not found at " + bin + "; run without --no-build or pass --bin");
  }
  if (mode === "local") {
    log("WARNING: SAS auto-confirm is enabled; run this only on an isolated test network");
    await localScenario(bin, options);
  } else if (mode === "serve") {
    await serveScenario(bin, options);
  } else if (mode === "join") {
    await joinScenario(bin, options);
  } else {
    usage();
    process.exitCode = 2;
    return;
  }
  log("PASS");
}

main().catch((error) => {
  const message = error && error.message ? error.message : String(error);
  process.stderr.write("[e2e] FAIL: " + message + "\n");
  process.exitCode = 1;
});
