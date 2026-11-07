# Contributing to Sayvane

Thanks for helping make private dictation better. You don't need to write Rust to make
a real difference: testing on your hardware is one of the most valuable contributions
right now.

## Ways to help

| Role | What it involves | Good first step |
| --- | --- | --- |
| 🧪 **Hardware tester** | Try Sayvane on Apple Silicon or Windows and report what works | Pick rows from the matrix in [`docs/compatibility.md`](docs/compatibility.md) and open an issue with your results |
| 🐛 **Bug reporter** | Tell us what broke, with steps to reproduce | Include OS version, chip (Apple Silicon / Intel / x64), model, and what you expected |
| 🎨 **Designer** | Polish the interface, floating bar, icons and empty states | Run `pnpm dev:mock` and iterate in the browser, with no backend needed |
| 🦀 **Rust developer** | Core features, platform adapters, performance | See "Good areas to start" below |
| ⚛️ **Frontend developer** | React pages, accessibility, keyboard navigation | `ui/src/pages/`, with tests in `ui/src/lib/` |
| 🌍 **Translator** | Localize the interface | Open an issue first; i18n scaffolding is still to come |
| 📝 **Writer** | Docs, guides and troubleshooting | Anything that confused you while setting up is worth fixing |

## Development setup

Follow **Run from source** in the [README](README.md#run-from-source). Short version:

```bash
pnpm install
scripts/build-worker.sh            # or: pwsh scripts/build-worker.ps1 on Windows
pnpm tauri dev
```

More detail, including pinned tool versions, is in [`docs/development.md`](docs/development.md).

## Before you open a pull request

```bash
pnpm test && pnpm build                       # interface: tests and strict type check
cd src-tauri && cargo test && cargo clippy    # core
```

- Keep pull requests focused: one change per PR is easiest to review.
- Add or update tests for behavior changes. Platform behavior that can't run in CI goes
  through the `Desktop` test double in `src-tauri/src/insertion.rs`.
- If you change an IPC type, update **both** `ui/src/ipc/types.ts` and
  `src-tauri/src/ipc_types.rs`.
- Match the surrounding code style. Run `cargo fmt` for Rust.

## Ground rules that protect users

These are product promises. Changes that weaken them won't be merged:

1. **Audio never leaves the computer.** No cloud speech recognition.
2. **Polishing stays opt-in.** With it off, no transcript text is sent anywhere.
3. **Never submit.** Dictation never presses Enter, Return or a send button.
4. **Save before acting.** Recordings and transcripts are persisted before polishing or
   pasting, so a failure never loses what the user said.
5. **Secrets stay in the OS vault.** Never in SQLite, settings files, logs or IPC responses.
6. **Neutral branding.** User-facing text says "speech engine" or "on-device", never the
   name of an underlying library.

## Good areas to start

- **Windows:** stop recording on sleep and screen lock (`WM_WTSSESSION_CHANGE`), and add
  start/finish sounds.
- **Apple Silicon:** benchmark Metal against CPU recognition and record results in
  `docs/compatibility.md`.
- **Localization:** set up translations for the interface.
- **Accessibility:** screen-reader pass on History and Settings.

## Reporting security issues

Please don't open a public issue for vulnerabilities. Open a GitHub security advisory on
this repository (**Security › Report a vulnerability**) so it can be fixed before
disclosure.

## License

By contributing, you agree that your contributions are licensed under the
[MIT License](LICENSE).
