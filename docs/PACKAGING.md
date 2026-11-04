# Packaging and signing (M5-04)

Plan 9.7 asks for installers and a clean-machine install. This document is the
release manager's runbook: what is produced, how to reproduce it, what is signed
and what is not, and what a clean-machine check has to prove.

## Android

`bun run android:build` builds the debug APK, which the Android debug certificate
signs, and checks the two properties a release needs: every shipped `.so` has
16 KB-aligned LOAD segments, and `apksigner` prints the signer.

~~~powershell
bun run android:build                      # debug, arm64-v8a
bun run android:build -- --target x86_64   # debug, emulator
bun run android:build -- --release         # release, needs a keystore
~~~

### Signing a release

A release build is signed by the Gradle project from
`src-tauri/gen/android/keystore.properties`. Without that file Gradle emits
`app-universal-release-unsigned.apk`, which no phone will install - the phone
says the package is invalid. `android:build --release` therefore refuses to start
when the file is missing, and refuses to finish if the APK still comes out
unsigned.

The Gradle side lives in `src-tauri/gen/android/app/build.gradle.kts`: a
`signingConfigs` block reads `storeFile`, `password` and `keyAlias` from that
properties file, and the release build type uses it only when the file exists, so
a checkout without a keystore still builds debug. Tauri generates that file, so a
fresh `tauri android init` replaces it - re-apply the block if the project is
regenerated.

~~~powershell
bun run android:keystore                   # once, then keep the backup
bun run android:build -- --release --target aarch64
~~~

The generator writes a 2048-bit RSA key with a 10,000-day validity to
`.android-secrets/fluxy-release.jks` and the matching properties file. The
password comes from `FLUXY_KEYSTORE_PASSWORD`, or a random one is generated; it
lands in the properties file, and both paths are gitignored.

Losing the keystore is not recoverable: Android treats a differently signed APK
as a different app, so an update can no longer be installed over the existing one.
Back up `.android-secrets/` and the password before shipping.

### Installing on a phone

~~~powershell
adb install -r app-universal-debug.apk
~~~

A debug APK is enough for testing and is already signed; the release path only
matters when preparing something to distribute.

## Artifacts

| Platform | Format | Command | Notes |
| --- | --- | --- | --- |
| Windows | NSIS `.exe` | `bun x tauri build --bundles nsis` | Per-user install, no elevation, English and Simplified Chinese |
| Windows | MSI | `bun x tauri build --bundles msi` | WiX, `en-US` and `zh-CN` |
| macOS | `.app` + `.dmg` | `bun x tauri build --bundles dmg` | Requires a macOS host and an Apple certificate for distribution |
| Linux | AppImage, deb | `bun x tauri build --bundles appimage,deb` | AppImage is the portable one |

Everything lands under `src-tauri/target/release/bundle/`. The release
binary itself is `src-tauri/target/release/fluxy.exe` on Windows;
`bun x tauri build --no-bundle` stops after that step, which is the fastest way
to check that the shell and the frontend still package together.

`scripts/package/windows.ps1` wraps the same commands and prints the version
resource and installer sizes, so a release log has the numbers in it.

### What this build produced

| Artifact | Size | Notes |
| --- | --- | --- |
| `target/release/fluxy.exe` | 10,537,472 bytes | FileVersion 0.2.0, ProductName Fluxy, CompanyName Fluxy contributors, copyright set from the bundle config |
| `target/release/bundle/nsis/Fluxy_0.2.0_x64-setup.exe` | 3,447,293 bytes | Authenticode: **NotSigned** |
| `target/release/bundle/msi/Fluxy_0.2.0_x64_en-US.msi` | 4.65 MiB | WiX; an administrative install (`msiexec /a ... TARGETDIR=<dir>`) extracts `PFiles/Fluxy/fluxy.exe` with the same 10,537,472 bytes |
| `target/release/bundle/msi/Fluxy_0.2.0_x64_zh-CN.msi` | 4.64 MiB | Same payload, Chinese UI strings |

The installer was exercised end to end on this machine: a silent install
(`/S /D=<dir>`) exited 0 and produced `fluxy.exe` plus
`uninstall.exe`, and a silent uninstall exited 0 and removed the directory.
That proves the package installs and uninstalls; it is not the clean-machine
check below, which still needs a host that never had Fluxy.

## What the bundle config sets

`src-tauri/tauri.conf.json` is the single source of truth:

- `publisher`, `copyright`, `category`, `shortDescription`,
  `longDescription` and `licenseFile` feed the installer metadata and the
  Add/Remove Programs entry. `licenseFile` points at the repository LICENSE so
  the MIT text shown by the installer cannot drift from the project.
- `windows.nsis.installMode` is `currentUser`: Fluxy is a per-user LAN
  tool, and a per-machine install would ask for elevation on a device where the
  user has no admin rights.
- `windows.nsis.languages` and `windows.wix.language` carry both UI
  locales the app ships (English and Simplified Chinese).
- Icons come from `src-tauri/icons/`, including the ICO for the Windows shell
  and the ICNS for macOS.

### Checksums

The build writes `target/release/bundle/SHA256SUMS.txt` next to the artifacts, and
the release notes quote it. Publish the file with the installers so a download can
be verified without trusting the release page's formatting.

## Signing

Nothing is signed by default: an unsigned build installs with a SmartScreen
warning, which is acceptable for development but not for a release.

**Windows.** Two supported paths, both configured in the same bundle section:

~~~text
# 1. A certificate in the Windows store (thumbprint)
$env:TAURI_BUNDLE_WINDOWS_CERTIFICATE_THUMBPRINT = '<sha1 thumbprint>'
# 2. A signing command (signtool, Azure Trusted Signing, ...)
$env:TAURI_BUNDLE_WINDOWS_SIGN_COMMAND = 'signtool sign /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 %1'
~~~

Then run the packaging script as usual; Tauri invokes the signer on the binary and
on the installers. Verify with `Get-AuthenticodeSignature` on each artifact.

**macOS.** Distribution needs a Developer ID certificate plus notarization:

~~~text
APPLE_CERTIFICATE, APPLE_CERTIFICATE_PASSWORD, APPLE_SIGNING_IDENTITY,
APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID
~~~

Tauri signs the bundle with the identity and submits it for notarization when
those variables are present.

**Linux.** No code signing; publish a `sha256sum` alongside the AppImage and
mention the checksum in the release notes.

## Offline and CI notes

- The first bundle on a machine downloads the NSIS and WiX toolchains into
  Tauri's cache. If the host cannot reach the download host, bundling fails while
  the release binary still builds: use `--no-bundle` in that case and
  package on a networked machine.
- WebView2 is installed on Windows 10 1803+ and Windows 11 by default. The
  installer keeps Tauri's default bootstrapper mode, so an older machine gets
  WebView2 downloaded during setup.

## Clean-machine checklist

The plan's acceptance for this milestone is "a clean machine can install". Run
this on a machine that has never had Fluxy:

1. Install the NSIS package. It must not ask for elevation, and it must appear
   under Settings > Apps with the publisher, version 0.2.0, and the MIT license.
2. Launch from the Start menu. The window opens, the tray icon appears, and the
   device name and id are shown on the device page.
3. Close the window. The process stays in the tray and a second instance (or the
   CLI) still reaches it.
4. Pair with another device on the same LAN and move one file.
5. Uninstall from Settings > Apps. The tray entry and the executable disappear.
   `%APPDATA%/com.thep0y.fluxy` keeps the identity, trust store, and config,
   which is deliberate: reinstalling must not orphan the user's pairings.
6. Reinstall. The device keeps its id and its trusted peers.

Record the result in the release notes; a failure here is a release blocker.
