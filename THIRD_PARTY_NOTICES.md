# Third-party notices

Sayvane bundles or downloads the following third-party components. Full license texts
for Rust crates and npm packages are generated at release time (see docs/release.md).

| Component | Use | License |
| --- | --- | --- |
| whisper.cpp / ggml (ggml-org) | On-device speech recognition engine inside the bundled `sayvane-speech` worker | MIT |
| OpenAI Whisper model weights (ggml conversions hosted by ggerganov/whisper.cpp on Hugging Face) | Speech models downloaded at the user's request | MIT |
| nlohmann/json | JSON protocol parsing in the speech worker | MIT |
| Tauri 2 and plugins | Desktop shell | MIT OR Apache-2.0 |
| React, React DOM | Interface | MIT |
| lucide-react | Interface icons | ISC |
| SQLite (via rusqlite, bundled) | Local History database | Public domain |
| cpal | Microphone capture (CoreAudio / WASAPI) | Apache-2.0 |
| symphonia | Audio file decoding for imports | MPL-2.0 |
| hound | WAV writing | Apache-2.0 |
| reqwest, tokio | HTTP for optional AI polishing and model downloads | MIT OR Apache-2.0 |
| keyring | OS credential vault access (Keychain / Credential Manager) | MIT OR Apache-2.0 |
| tauri-nspanel | Non-activating floating bar on macOS | MIT OR Apache-2.0 |

"Whisper" is a name used by OpenAI for its speech model. Sayvane is an independent
product and is not affiliated with or endorsed by OpenAI or the ggml project.

The speech worker's own notices are in `speech-worker/NOTICE.md` (MIT texts for whisper.cpp/ggml, the OpenAI Whisper model weights, and nlohmann/json).
