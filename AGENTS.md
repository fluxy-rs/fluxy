# Fluxy — agent notes

## Running the desktop app

Never launch `target/debug/fluxy.exe` on its own. A debug build resolves its UI
through `devUrl` (`http://localhost:8563`), so without the Vite dev server the
window opens blank and every UI observation is wrong.

| Goal | Command |
| --- | --- |
| Develop, hot reload, real UI | `bun run dev` |
| A standalone app with the UI embedded | `bun run build` (then `target/release/fluxy.exe`) |
| Only the frontend dev server | `bun run start` |

If a debug binary is already running and the UI is blank, the dev server is
missing or dead — start `bun run dev` and restart the app.

## Android

```powershell
bun run android:keystore                  # once, for release builds
bun run android:build                     # debug APK, arm64
bun run android:build -- --release        # signed release (needs the keystore)
```

Keep `cargo check --target …` away from the target directory that
`tauri android build` uses, or ring's build script caches incompatible state and
the APK dies at startup with a missing `ring_core_…_vaes_avx2` symbol. Use
`--target-dir target/check-android` for hand-run checks.

### Never `android:build` to verify something on the phone

**`bun tauri android dev` is the only way to check a change on the device.**
It rebuilds the Rust side, installs, and hot reloads the frontend. `bun run
android:build` is for one thing only: producing an APK to hand to someone else,
or to measure transfer performance on a release build. It is never how an agent
verifies its own work.

This has been got wrong four times. Each time it cost 5-10 minutes to build,
another install step, and a window where the device ran stale code. The rule is
not advisory:

| You want to | Run |
| --- | --- |
| See a UI change on the phone | `bun tauri android dev` (already running? just edit - it hot reloads) |
| See a Rust change on the phone | restart `bun tauri android dev` - it rebuilds and installs |
| Hand someone an installable app | `bun run android:build -- --target aarch64` |
| Measure throughput on a release profile | `bun run android:build -- --release` |

If `tauri android dev` is already running in a background job, **do not start a
second one and do not build an APK**. Restart that job, wait for the device to
come back, then look at it.
### Builds for UI work on the phone

A debug APK still embeds the frontend. `bun run android:build` is a *build*, not
a dev server hookup, so the WebView loads `http://tauri.localhost/assets/...`
from inside the APK - check logcat and you will see it. A UI edit therefore needs
a rebuild in debug just as much as in release:

```powershell
bun run android:build -- --target aarch64      # debug: faster, unsigned
adb -s <device> install -r -t <debug apk>
```

Dev-server loading only happens under `tauri android dev`, and that path **does work** -
logcat shows the WebView loading `http://tauri.localhost/src/pages/...` from the dev
server. An earlier note here claimed it was blocked; it was tried once, failed,
and the conclusion stuck long after it stopped being true. Prefer it for UI work:
`bun tauri android dev` rebuilds the native side and then hot-reloads frontend edits.

**Debug and release APKs have different signatures.** `adb install -r` between
them fails with `INSTALL_FAILED_UPDATE_INCOMPATIBLE: signatures do not match`,
and the failure is easy to miss because `install` returning nothing still looks
like success in a piped command. Uninstall first, and check
`dumpsys package <id> | grep DEBUGGABLE` afterwards to confirm which one is
actually installed. Note that uninstalling wipes the address book.

`--host` exists for mobile dev (`bun tauri android dev --host <addr>`) and is
the documented way to tell the dev server which address the phone should use.
It fails before it gets there on adb 37.0.1: the CLI probes the device with a
bare `adb -s <serial>`, which prints help and exits 1.

### The barcode scanner plugin is vendored, and for good reason

`plugins/tauri-plugin-barcode-scanner` is a local copy. The upstream 2.5.3 build
does not work on a device without Google Play Services, and it fails silently in
several different ways. Five separate defects, all upstream:

1. It depended on `play-services-mlkit-barcode-scanning` (unbundled). Without a GMS
   dynamite module the log says `No acceptable module ...vision.dynamite found` and
   the camera opens but never decodes. Fixed by depending on
   `com.google.mlkit:barcode-scanning` in **the plugin's own** gradle - doing the swap
   in the app module instead drops `vision-interfaces` and the app dies with
   `Failed resolution of: ...BarcodeScannerOptions$Builder`.
2. `mapFormats` allocated an array and skipped the QR entry, leaving a literal `0`
   where a `Barcode.FORMAT_*` belongs. MLKit then filtered out every code and
   reported no error.
3. `setBarcodeFormats(Barcode.FORMAT_ALL_FORMATS)` throws: `-1` is not a valid
   format, and "all formats" is expressed by not calling the method at all.
4. `analyze()` called `scanner?.process(...)`; when scanner was null the whole
   chain short-circuited, including `image.close()`. One unclosed frame stalls
   `STRATEGY_KEEP_ONLY_LATEST` forever, so the camera looked frozen with no logs.
5. `configureCamera()` threw when the preview was null, and `windowed: true` puts
   the preview *behind* the WebView and makes the WebView transparent - unusable
   when the page paints an opaque background. Scanning is full-screen now.

Diagnostics live under the `FluxyScan` log tag: one line per analysed frame, one per
MLKit result. Use `adb logcat -s FluxyScan` before guessing.

### Hardware identification is a crate, not a plugin

`crates/fluxy-sysinfo` answers brand / model / OS version for the pairing list.
It is a plain crate rather than a Tauri plugin because the values have to reach
`fluxy-net`: they travel in the mDNS TXT record and in the handshake, and a
plugin is not reachable from the network layer.

Per platform: the registry (`HARDWARE\\DESCRIPTION\\System\\BIOS` plus
`Windows NT\\CurrentVersion`) on Windows, `/sys/class/dmi/id/*` and
`/etc/os-release` on Linux, `__system_property_get` (`ro.product.*`,
`ro.build.version.*`) on Android, `sw_vers`/`sysctl` on macOS.

Three things to keep in mind:

- The extra Hello/HelloAck fields (10-12) are **outside** the signed transcript.
  `hello_transcript` lists its fields explicitly, so adding them cannot break an
  older peer's signature check, and protobuf drops unknown fields on the way in.
- The UDP gateway packet has a fixed byte layout; its `PeerAdvertisement` carries
  empty hints. Only mDNS transports them.
- Vendor strings are trimmed of NULs and whitespace before use;
  empty means "unknown" and the UI must degrade rather than print a blank row.

### A Rust change is not in effect until the APK is rebuilt

`cargo check`, `clippy` and `cargo test` all compile for the **host**. The phone
only ever runs `libfluxy_lib.so` from inside the APK, so **every** Rust edit that
matters on the device needs:

```powershell
bun run android:build -- --target aarch64        # rebuild
adb -s <device> install -r -t <apk>              # install
```

When the desktop app is running, `cargo test` cannot link: it fails with
`failed to remove target/debug/fluxy.exe (os error 5)` and the run reports
`passed=0 failed=0`, which reads like a passing suite if only the counts are
checked. Either stop the desktop first or run with
`--target-dir target/verify`.

Before drawing any conclusion from a phone test, compare the APK's mtime with the
file you edited. A green build proves the code compiles, never that the device is
running it. This has been the cause of five wasted verification rounds.

The desktop equivalent is `bun run build`; if it fails with
`拒绝访问 (os error 5)`, a previous `fluxy.exe` is still running and must be
stopped first.

### Debugging a release profile

The dev server must be up **before** the app, because both flavours load their
frontend from it:

```powershell
bun run dev:web                    # binds 0.0.0.0 so the phone can reach it
bun run tauri dev --release        # desktop, release Rust + dev UI
bun run tauri android dev --release # phone, release Rust, debug signing
```

Do not start the app first: it will open a blank window (desktop) or fail to
fetch the page (phone). `bun run dev:web` is idempotent — it reuses a server that
is already listening.

### Drive the phone yourself

Verification is the agent's job, not the user's. Everything reachable through
`adb` should be done through `adb`: `am start` to launch or deliver a share
intent, `input tap`/`input text` to drive the UI, `content query` to resolve a
MediaStore URI, `logcat` to read the result.

Pick every tap coordinate from a **fresh screenshot**, never from a hard-coded
value — the layout shifts as lists reorder, and a tap that lands one row off has
repeatedly produced wrong conclusions. Take the screenshot, read the pixel
position of the target, then tap.

Ask the user only for what `adb` cannot do: unlocking the device, pairing by
scanning a code, or judging a subjective result.

### Drive the desktop too (WebView2 debug port)

The desktop UI has no accessibility driver, but the WebView2 port gives the same
reach as `adb` does for the phone. Start the app with the port enabled:

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'
bun run dev -- --release
```

Then `http://127.0.0.1:9222/json` lists the page and its
`webSocketDebuggerUrl`. Connect, send `Runtime.evaluate` with
`awaitPromise: true, returnByValue: true`, and the Tauri commands are reachable
through the bridge the bindings use internally:

```js
window.__TAURI_INTERNALS__.invoke("start_transfers", { deviceId, fileIds })
```

Two things to know. Command names are **snake_case** (`state_snapshot`,
`register_file`, `start_transfers`), not the camelCase keys in
`src/bindings.ts`. And `window.__TAURI__` does not exist - only
`__TAURI_INTERNALS__` does. For a quick smoke check:

```js
window.__TAURI_INTERNALS__.invoke("state_snapshot").then(s => s.status.device_name)
```

This is how the desktop-to-phone transfer in the T-M02 run was driven without a
human at the keyboard.

### The generated Android project is not tracked

`src-tauri/gen/android` is produced by `tauri android init` and has never been
committed (`git status` shows it as untracked, not ignored). Anything edited
there survives until the project is regenerated, then disappears without a
warning. Two things had already been hand-written into
`app/src/main/AndroidManifest.xml` this way: `android:roundIcon` and the
`TransferService` `<service>` entry.

Put Android manifest changes in
`plugins/tauri-plugin-fluxy-mobile/android/src/main/AndroidManifest.xml`
instead. Gradle merges it into the app manifest at build time, so it survives
regeneration. Verify the merge rather than the source file:

```powershell
adb -s <device> shell dumpsys package com.thep0y.fluxy | Select-String WAKE_LOCK
```

### Known bug: a Rust mutex destroyed while another thread locks it

Seen once, on 2026-11-03 at 21:55:08, in the Android build:

```text
F libc: FORTIFY: pthread_mutex_lock called on a destroyed mutex (0x7d98d1ce58)
```

Two threads hit it within the same millisecond and the process died. It has not
reproduced since: four rounds of background/foreground cycling and three full
transfers on the same build were clean, and the crash buffer had already been
cleared, so there is no stack.

This is a use-after-free in Rust, not a Kotlin problem. It is a plausible
explanation for two other mysteries - a transfer that stops mid-write, and a
chat session that ends on its own after roughly 40 s in the background - so if
either shows up again, suspect this first. Do not guess at a fix: get a stack
first. `adb logcat -b crash` only has the JVM side; a native abort needs
`/data/tombstones` (root) or `adb bugreport`.

### A foreground service that dies 7 ms after starting

`TransferService.onStartCommand` logged `startForeground` and the process was
killed anyway, with `ForegroundServiceDidNotStartInTimeException`. The
`startForeground` call had succeeded; the crash came from the next line,
`wakeLock.acquire()`, which throws `SecurityException` when
`android.permission.WAKE_LOCK` is not in the manifest. An exception thrown out
of `onStartCommand` makes the system conclude the service never entered the
foreground.

So: a `ForegroundServiceDidNotStartInTimeException` does not always mean slow
startup. Read what `onStartCommand` does *after* `startForeground` - any
uncaught exception there produces the same message.

## Branching: never commit to main

`main` is protected by convention, not by GitHub settings. Nobody - human or
agent - pushes to it directly. Every change, however small, goes through a
branch and a pull request.

```powershell
git switch main
git pull --ff-only
git switch -c fix/transfer-offer-dismiss
# ...edit, run the gates below, commit...
git push -u origin fix/transfer-offer-dismiss
gh pr create --base main --fill
```

Names say what the branch does: `fix/`, `feat/`, `docs/`, `chore/`, followed by
a few words about the change. Avoid `wip`, `test`, or a person's name.

Three things this protects against, all of which have bitten this project:

- A broken commit on `main` cannot be verified against the release build.
- Force-pushes to a shared branch erase other people's work with no review step.
- The gates in the next section are a checklist for a pull request, not for a
  push nobody reads.

Running the gates on the branch before opening the PR is part of the job, not
something the reviewer does.

## Commits: conventional, and saying why

Every commit message follows Conventional Commits, no exceptions:

```text
<type>(<scope>): <what changed>

<why it was needed>
```

The subject is imperative, lower case, no trailing full stop, and fits in 72
characters. `fix(ci): stop the workflow failing before it tests anything` -
not `Fixed CI`, not `update ci.yml`.

| Type | Use it for |
| --- | --- |
| `feat` | a capability a user can see |
| `fix` | behaviour that was wrong |
| `perf` | measurably faster or lighter |
| `refactor` | same behaviour, different shape |
| `test` | tests and fixtures only |
| `docs` | prose, comments, this file |
| `build` | dependencies, packaging, release plumbing |
| `ci` | workflows, gates, tooling |
| `chore` | anything else that touches no product code |

The scope names the area in lower case: `ci`, `net`, `transfer`, `desktop`,
`test`, or a crate. It is optional; the type is not.

**The body is not optional.** It states why the change exists — what was broken,
what it cost, what was ruled out. "Update the file" is not a reason: the diff
already shows the file changed. A reader six months from now has the code and
the diff, and neither explains motive.

```text
fix(net): recognise lo0 as loopback

macOS reports no interface type through getifaddrs, so lo0 arrives as
InterfaceKind::Other. is_loopback only accepted the kind or the name "lo",
which is the Linux spelling, so the macOS job failed on "every machine has a
loopback interface".
```

Three habits this rules out, all of them already in this repository's history:

- A subject with no prefix, which makes `git log --oneline` useless for
  scanning.
- A body that restates the diff: it costs a line and buys nothing.
- One commit holding several unrelated changes. Split them so a reviewer can
  reject one without unpicking the others.

`.githooks/commit-msg` enforces the shape. Enable it once per clone:

```powershell
git config core.hooksPath .githooks
```

## Pull requests: one purpose, split into commits

A pull request answers exactly one question. The test is to write its
description first, in one sentence, without "and". If the sentence needs a
second clause starting with "also", it is two pull requests.

```text
make CI green                       one thing
add file-type icons to transfers    one thing
make CI green and add file icons    two things
```

"It touches three files" is not a reason to split. "It fixes three unrelated
problems" is. The unit is the reason, not the diff size.

**Inside that one purpose, commits are steps.** A reviewer reads them one at a
time and should be able to follow the argument: this was broken, here is the
proof, here is the repair. One commit that changes forty files is not a step,
it is a cliff — there is nowhere to stop and agree.

Split by what can be understood and reverted on its own. A refactor that makes
the fix possible is its own commit, ahead of the fix. A test that proves a bug
comes before the repair that makes it pass, so the failing run stays in the
history rather than being described in prose.

### When something unrelated turns up mid-task

Another branch, another pull request. Not a convenience commit tacked onto the
current one, and not "I was already in the file". The cost of the extra PR is a
few minutes; the cost of a mixed one is a reviewer who cannot reject half of it.

### When the second change depends on the first

Stack them rather than merging the two purposes:

```powershell
git switch -c feat/the-second-thing feat/the-first-thing
# ...edit...
git push -u origin feat/the-second-thing
gh pr create --base feat/the-first-thing
```

The second pull request shows only its own diff. Once the first lands and its
branch is deleted, GitHub retargets the second at `main` on its own.

## Frontend files: one reason to change

The complaint this section answers is real: `ChatWorkspace.tsx` reached 1,200
lines by accumulating a timeline, a context menu, three dialogs and a forward
picker. Nothing below is about tidiness for its own sake - it is about a reader
being able to name what a file does before opening it.

### Shape

```text
src/
  pages/            a route: composition, no business state
  components/       one directory per component
    ChatWorkspace/
      index.ts            exports only
      ChatWorkspace.tsx   JSX and child composition
      useChatWorkspace.ts state, derived values, handlers, effects
      ChatWorkspace.types.ts
      ChatWorkspace.utils.ts   pure functions
      ChatWorkspace.consts.ts  constants and literal tables
      ChatWorkspace.test.tsx   tests sit next to the code
  state/            providers and the engine bridge
  api/              typed wrappers over the generated bindings
  lib/              framework-free helpers
  ui/               vendored neut/ui, not ours to restyle
```

The split into seven files is a **ceiling, not a template**. A presentational
component with no state needs `index.ts` and `<Name>.tsx` and nothing else;
create `use<Name>.ts` when there is state, `.utils.ts` when there is a pure
helper worth testing alone. Empty files that exist to match a diagram are the
same cost as a 1,000-line one, just spread out.

### Rules

1. **One exported component per file**, named after the file.
2. **`.tsx` renders and composes.** No business decisions, no `fetch`/`invoke`,
   no cross-module side effects. Local UI state is allowed until rule 3 says
   otherwise; a hover flag does not need a hook.
3. **Extract a hook when either is true**: the component exceeds 120 lines, or
   it owns three or more pieces of state. Below that, splitting teaches a
   reader to jump between files for nothing.
4. **Props are never destructured** - Solid props are a reactive proxy and
   destructuring freezes them. Read `props.x` in JSX and inside hooks.
5. **A controlled prop is read, not copied.** `const value = () => props.value
   ?? internal()` - never `createEffect` that writes a prop into a signal; that
   costs a frame, fights the user, and can loop.
6. **Derived values use `createMemo`**, not `createEffect` plus a signal.
7. **Handlers are named** and returned from the hook, so a test can call them
   without simulating a pointer.
8. **Dependencies point one way**: `pages -> components -> hooks/lib`. A
   component never imports a page, and no directory reaches into another
   feature's internals.
9. **Copy comes from the catalogue** and styles from Tailwind tokens +
   `.styles.ts`; no literal Chinese in components, no hex colours.
10. **Async surfaces ship all three states**: loading, error with a retry, empty.

### Hard limits, and why they are ratcheted

| metric | limit |
| --- | --- |
| `.tsx` file | 250 lines |
| `.ts` file | 300 lines |
| one function body | 120 lines |
| nested JSX elements | 6 |

Line counts are a proxy, and a crude one - 200 lines of flat JSX read better
than 140 lines of nested ternaries. They are used because they are the one
property a machine can hold honestly, and because they catch the failure that
actually happened here: a component growing every time someone needed one more
thing. The judgement stays with the reviewer under the heading above.

`bun run check` enforces them through `scripts/check-code-limits.mjs`, which
measures with a real parser (Babel), not by counting lines of text:

```powershell
bun run check:limits          # what CI runs
bun run check:limits --update # regenerate the baseline after a split
```

Files already over the limit are listed in `scripts/code-limits-baseline.json`
at their current size. **The baseline may only shrink**: a listed file cannot
grow, a file that is not listed must fit, and splitting a file by hand removes
its entry the next time someone runs `--update`. Without that ratchet the rule
would freeze the repository on the day it landed; with it, the number only
goes down.

Two files are outside the count, and the reason is stated in the script:
`src/i18n/index.tsx` is a translation table, and `src/bindings.ts` is generated.
`src/ui/**` is vendored from neut/ui and keeps upstream's shape - it is still
measured, so a copy may not grow, but it is not restyled to suit us.
## Gates before calling something done

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --exclude udp-chaos
bun run check
bun run test:frontend
bun run check:security
bun run check:docs
```

`udp-chaos` is excluded here for the same reason it is excluded in CI: its
fault-injection cases assert on chat latency and on reconnect timing, which a
shared machine does not hold steady. Run `cargo test -p udp-chaos` alone when
you touch the transport.

Bindings come from `FLUXY_EXPORT_BINDINGS=1 cargo run -q -p fluxy`; never edit
`src/bindings.ts` by hand. UI copy lives in `src/i18n/index.tsx` only, with zh and
en kept in step.
