# Release

## Owner inputs required

| Input | Used for |
| --- | --- |
| Final `applicationId` / `storageNamespace` under a domain you control (currently the development value `app.sayvane.desktop`) | Data directory, Keychain/Credential Manager entries, bundle ID. Freeze before the first public build. |
| Apple Developer ID Application certificate + notarization credentials | Signing and notarizing both macOS DMGs |
| Windows code-signing certificate (optional but recommended) | Signing the NSIS installer and executables |
| Trademark clearance for "Sayvane" | Name checks so far: GitHub, npm, crates.io, Mac App Store, web search — no conflicts found. This is not a legal trademark search. |

## Steps

1. Merge to `main`. The `build` workflow builds the Apple Silicon DMG, the Intel DMG and
   the Windows x64 installer (each with architecture checks), then publishes the next
   version automatically: it creates the `vX.Y.Z` tag and a GitHub release holding
   `Sayvane-macOS-AppleSilicon.dmg`, `Sayvane-macOS-Intel.dmg`,
   `Sayvane-Windows-x64-Setup.exe`, `Sayvane-source.zip` and `SHA256SUMS.txt`.
   - The version is the newest tag with its patch number plus one. To release a new minor or
     major version, raise `version` in `src-tauri/tauri.conf.json` (for example to `0.2.0`).
   - Changes that only touch `site/`, `docs/` or Markdown files do not build or release.
   - Add `[skip release]` to a commit message to build without releasing.
   - The website links to `releases/latest/download/<file>`, so downloads always serve
     the newest release with no site change.
2. macOS signing: set `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`,
   `APPLE_TEAM_ID` and run `pnpm tauri build --target <triple>`. Tauri signs nested
   binaries (including `sayvane-speech`) before the app, then notarizes.
3. Windows signing: configure `bundle.windows.certificateThumbprint` or a signing
   command, then rebuild.
4. Install each artifact on a clean machine (no compiler, Node, Python, Homebrew or
   checkout), install a model, disconnect the network, and dictate. Record results in
   `docs/compatibility.md`.
5. Generate full license texts:
   `cargo about generate` (Rust) and `pnpm licenses list --prod` (npm), and append
   them to `THIRD_PARTY_NOTICES.md`.

Never mark signing or clean-machine verification as done unless it happened.
