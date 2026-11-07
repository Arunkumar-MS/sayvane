# Compatibility and verification record

Status as of 7 November 2026. "Verified" means observed on real hardware; CI and cross
compilation alone are listed separately and never counted as hardware verification.

## Build matrix

| Target | Worker | App | Where verified |
| --- | --- | --- | --- |
| macOS Intel (`x86_64-apple-darwin`) | Built, 61/61 smoke checks with real model | Built and launched (dev) | Intel MacBook, macOS 15.7, 8 GB |
| macOS Apple Silicon (`aarch64-apple-darwin`) | Cross-compiled only (old SDK); Metal not run | Not built | CI job `macOS Apple Silicon` (pending first push) |
| Windows x64 (`x86_64-pc-windows-msvc`) | Not built | Not built | CI job `Windows x64` (pending first push) |

Decisions that affect support:

- Audio capture uses cpal 0.15.3 because cpal 0.16+ requires macOS 14.2 for CoreAudio.
- Intel macOS worker is compiled for AVX2 (all Macs that run macOS 13 have it); it
  reports `unsupported_cpu` instead of crashing under Rosetta on older macOS.
- Windows worker defaults to SSE4.2 for broad Windows 10 compatibility; build with
  `-CpuLevel avx2` for a faster variant.
- Model weights are not bundled; installs download on request (SHA-256 verified) or
  import a local file for offline machines.

## Automated coverage (this machine)

- Rust core: 58 tests — gesture timing, settings recovery, migrations, interrupted
  capture repair, retry derivation, exports, statistics across local midnight,
  provider URL rules / profile isolation / origin binding / error mapping, insertion
  safety (password refusal, app switch, cancel before paste, single paste, guarded
  clipboard restore), resampling, decoding, and real recognition through the worker.
- UI: 17 tests — status mapping, date grouping, accelerator capture, formatting.

## Manual matrix

Fill in on each machine before a release. Only the first rows have been observed.

| Scenario | Apple Silicon | Intel Mac | Windows x64 |
| --- | --- | --- | --- |
| Launch from dev build; main window, tray, floating bar present | — | Observed | — |
| Floating bar at floating level, bottom-center above Dock | — | Observed | — |
| Install and launch without development dependencies | — | — | — |
| Local recognition with network disconnected | — | — | — |
| Platform shortcut and configurable alternative | — | — | — |
| Non-activating floating bar (focus stays in target) | — | — | — |
| Browser / native editor / Electron insertion | — | — | — |
| Unicode and multiline insertion; selection replacement | — | — | — |
| App or window change during recognition | — | — | — |
| Password field and ineligible target | — | — | — |
| Copy during delayed clipboard restore | — | — | — |
| Multiple displays and scale changes | — | — | — |
| Microphone denial and device unplug | — | — | — |
| Sleep or lock during recording | — | — | — |
| Crash recovery and explicit retry | — | — | — |
| OpenRouter success and offline fallback | — | — | — |
| Custom compatible and loopback profile | — | — | — |
| Elevated destination and secure desktop | n/a | n/a | — |

## Known gaps

- Windows: sleep/lock does not yet stop capture proactively (device errors still do).
- Windows: the start/finish sounds are silent.
- Clipboard restoration on Windows skips GDI-handle formats (bitmaps, metafiles) and
  reports the snapshot as incomplete.
