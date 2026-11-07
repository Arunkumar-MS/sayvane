# Development

Sayvane is a Tauri 2 desktop app: a React + TypeScript interface (`ui/`), a Rust core
(`src-tauri/`), and a bundled C++ speech worker (`speech-worker/`) built on whisper.cpp.

## Pinned toolchain

| Tool | Version |
| --- | --- |
| Node | 22.18.0 |
| pnpm | 10.28.0 |
| Rust | 1.99.1 (MSRV 1.89, set by rusqlite) |
| CMake | 4.4.5 (≥ 3.21 works) |
| whisper.cpp | v1.9.5 (tarball SHA-256 pinned in `speech-worker/CMakeLists.txt`) |
| Tauri | 2.12.5 (CLI 2.12.4) |

macOS builds need the Xcode Command Line Tools; Windows builds need Visual Studio 2022
Build Tools (MSVC, Windows 11 SDK) and WebView2 (preinstalled on Windows 11).

## First run

```bash
pnpm install
scripts/build-worker.sh            # macOS; Windows: pwsh scripts/build-worker.ps1
node scripts/generate-branding/index.mjs
MACOSX_DEPLOYMENT_TARGET=13.0 pnpm tauri dev
```

`pnpm dev:mock` runs the interface in a browser against an in-memory mock core, which
is the fastest way to work on visuals. It cannot exercise native behavior.

## Tests

```bash
pnpm test                                    # UI unit tests
cd src-tauri && cargo test                   # core tests
SAYVANE_TEST_MODEL=/path/ggml-tiny.en.bin cargo test   # adds real recognition
python3 -I speech-worker/tests/smoke.py --worker src-tauri/binaries/sayvane-speech-<triple> --model <model>
```

Platform adapters are tested through the `Desktop` test double in
`src-tauri/src/insertion.rs`; native focus and insertion behavior must be checked on
real hardware (see `docs/compatibility.md`).

## Layout

| Path | Contents |
| --- | --- |
| `branding/` | `brand.json` and the icon source — the only place product identity lives |
| `ui/src/ipc/` | IPC contract (`types.ts`, `commands.ts`) and the browser mock |
| `src-tauri/src/session.rs` | The single dictation state machine |
| `src-tauri/src/core.rs` | Storage, settings, events, startup reconciliation |
| `src-tauri/src/insertion.rs` | Shared delivery procedure (paste once, guarded restore) |
| `src-tauri/src/platform/` | macOS (AX, NSPasteboard, CGEvent tap) and Windows (UIA, SendInput) adapters |
| `src-tauri/src/speech/` | Worker client and verified model manager |
| `src-tauri/src/providers.rs` | Optional AI polishing over compatible chat APIs |
| `speech-worker/` | Recognition worker; protocol in its README |

## Rebranding

Edit `branding/brand.json` and `branding/icon.svg`, then:

```bash
node scripts/generate-branding/index.mjs --icons
```

Keep `applicationId` and `storageNamespace` unchanged after a public release: they key
the data directory and the credential vault entries.

## macOS permissions during development

Each rebuild of an unsigned dev binary can look like a new app to macOS, so
Accessibility may need to be re-granted (System Settings › Privacy & Security ›
Accessibility: remove and re-add the app).
