# sayvane-speech: local speech worker

`sayvane-speech` is the bundled native worker that runs local speech
recognition for Sayvane. The Rust core starts it as a Tauri sidecar and talks
to it over stdin/stdout. Internally it statically links **whisper.cpp v1.9.5**
(MIT) and its ggml library. That name belongs in technical docs and
[NOTICE.md](NOTICE.md) only. Product UI and worker error messages use neutral
wording ("speech engine", "local recognition").

## Pinned dependencies

| Dependency | Pin | Integrity |
| --- | --- | --- |
| whisper.cpp | `v1.9.5`, `https://github.com/ggml-org/whisper.cpp/archive/refs/tags/v1.9.5.tar.gz` (FetchContent) | SHA-256 `ff1a9053feb509ff9d7729703355541ae9690073a6b1c40eb692c962e0dc1720` |
| nlohmann/json | `v3.12.0` single header, vendored at `vendor/nlohmann/json.hpp` | SHA-256 `aaf127c04cb31c406e5b04a63f1ae89369fccde6d8fa7cdda1ed4f32dfc5de63` |
| CMake | 3.21 or newer (verified with 4.4.5) | |
| macOS toolchain | Xcode / Command Line Tools. Release builds should use a current Xcode SDK; verified locally with Apple clang 12 and the macOS 11.0 SDK | |
| Windows toolchain | Visual Studio 2022 (MSVC v143) Build Tools, x64 or ARM64 | |

Model weights are not part of the build. See `models/manifest.json`, which
lists URLs, sizes and SHA-256 digests from Hugging Face `ggerganov/whisper.cpp`.

## Building

Each script writes a Tauri sidecar binary named for its target triple to
`src-tauri/binaries/`.

```sh
# macOS (host triple by default)
scripts/build-worker.sh                      # -> sayvane-speech-<host-triple>
scripts/build-worker.sh aarch64-apple-darwin # Apple Silicon, Metal + CPU
scripts/build-worker.sh x86_64-apple-darwin  # Intel, CPU only
```

```powershell
# Windows x64 (Developer PowerShell for VS 2022)
pwsh scripts/build-worker.ps1                 # -> sayvane-speech-x86_64-pc-windows-msvc.exe
pwsh scripts/build-worker.ps1 -CpuLevel avx2  # optional faster, less compatible build
```

`build-worker.sh` finds CMake from `$CMAKE`, then `PATH`, then
`$HOME/.local/cmake-bin/cmake`. After a build, both scripts check the binary.
The shell script checks `lipo -archs`, `file` and `otool -L`, which must show
system libraries only. The PowerShell script checks the PE machine type and
runs `dumpbin /dependents`. Neither VC++ runtime DLLs nor OpenMP DLLs are allowed.

Optional environment variables:
- `SAYVANE_ENGINE_TARBALL` points to a local copy of the pinned engine tarball
  for offline builds. The SHA-256 is still verified.
- `SAYVANE_X86_LEVEL` sets the x86 CPU level (see below).
- `SAYVANE_DEPLOYMENT_TARGET` sets the macOS deployment target (default 13.0).
- `SAYVANE_EXTRA_CMAKE_ARGS` passes extra arguments to the CMake configure step.

Raw CMake also works:
`cmake -S speech-worker -B speech-worker/build/x -DCMAKE_OSX_ARCHITECTURES=arm64 && cmake --build speech-worker/build/x --target sayvane-speech`.

### Build configuration decisions

- **Static engine.** `BUILD_SHARED_LIBS=OFF`, so whisper and ggml link into
  the single executable and no extra dylibs or DLLs ship. `GGML_OPENMP=OFF`,
  because OpenMP would add `vcomp140.dll` or `libomp`; ggml's own thread pool is
  used instead. CUDA, Vulkan, HIP, SYCL, OpenCL and WebGPU are all off.
- **MSVC runtime.** Builds use `/MT` (`CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded`,
  with policy CMP0091 forced on for the engine subproject), so no VC++
  redistributable is needed.
- **macOS.** `CMAKE_OSX_DEPLOYMENT_TARGET=13.0`. Accelerate (a system
  framework) is used for BLAS on both architectures.
  - arm64: `GGML_METAL=ON` and `GGML_METAL_EMBED_LIBRARY=ON` (no separate
    `.metallib` resource). The CPU code is compiled for
    `-march=armv8.4-a+fp16+dotprod`, which every M1 or later supports.
  - x86_64: `GGML_METAL=OFF`. Intel Macs always use the CPU. Metal is not
    enabled just because the OS is macOS.
- **x86 CPU level (`SAYVANE_X86_LEVEL`).** ggml has runtime CPU-variant
  dispatch (`GGML_CPU_ALL_VARIANTS`), but in v1.9.5 it requires
  `GGML_BACKEND_DL`. That mode builds each variant as a shared library
  loaded with `dlopen`/`LoadLibrary`, which conflicts with shipping one static
  executable. Instead, one fixed level is compiled in with `GGML_NATIVE=OFF`:
  - `avx2`, the **default for x86_64-apple-darwin**: AVX2, FMA, F16C and BMI2
    (Haswell or newer). macOS 13 runs only on 2017-or-later Intel Macs, and all
    of them have these instructions.
  - `sse42`, the **default for x86_64-pc-windows-msvc**: SSE4.2 only, no AVX.
    Windows 10 still runs on CPUs without AVX (older Pentium, Celeron and Atom
    parts), so this level is the conservative choice. Building with
    `-CpuLevel avx2` gives a faster binary for modern PCs, but that binary does
    not run on these older CPUs.
  - `native` tunes for the build machine. Use it only for local experiments.
  - **Startup guard:** `src/cpu_check.cpp` is compiled without SIMD flags. At
    startup it checks CPUID and XCR0 against the compiled level. If the CPU
    lacks a required instruction, `hello` reports `cpuSupported:false`,
    `capabilities.backends` is `[]`, and `warm`/`transcribe` return
    `unsupported_cpu`. An illegal-instruction crash cannot happen. This matters
    for the macOS x86_64 build under Rosetta 2 on macOS 13 and 14, which does
    not emulate AVX.

## Protocol (version 1)

The protocol uses newline-delimited UTF-8 JSON. Commands arrive on stdin and
responses leave on stdout, one object per line, flushed after each message.
Diagnostics go only to stderr; set `SAYVANE_SPEECH_VERBOSE=1` for engine info
logs. The parent must drain stderr.

- Every message carries `"protocolVersion": 1` and `"requestId"` (1–256
  bytes). Every response carries `type` and `ok`, and echoes `requestId`.
  Unparseable lines get `requestId: null`.
- Lines larger than **1 MiB** are discarded and answered with `bad_request`.
- Paths (`modelPath`, `audioPath`) must already be resolved and validated by
  the trusted core. The worker only checks that each one is a readable
  regular file. Paths are UTF-8, including on Windows.

### Commands and responses

| Command | Fields | Success response |
| --- | --- | --- |
| `hello` | | `{"type":"hello","worker":"sayvane-speech","workerVersion","engineVersion":"1.9.5","arch":"x86_64"\|"aarch64","cpuLevel","cpuSupported"}` |
| `capabilities` | | `{"type":"capabilities","backends":["metal","cpu"]\|["cpu"],"arch","engineVersion","maxAudioSeconds":7200,"supportsTranslate":true,"supportsWordTimestamps":true,"supportsBeamSearch":true,"maxBeamSize":8,"maxLineBytes","options":[...],"cpuLevel","cpuSupported","loadedModel":null\|{"modelPath","backend","multilingual","canTranslate"}}` |
| `warm` | `modelPath`, `backend` (`auto`\|`cpu`\|`metal`, default `auto`) | `{"type":"warmed","modelPath","backend":"metal"\|"cpu","multilingual","canTranslate","alreadyLoaded","loadMs"}` |
| `transcribe` | `jobId`, `generation` (integer ≥ 0), `modelPath`, `audioPath`, `backend`, `options` | zero or more `{"type":"progress","jobId","generation","percent":0-100}` messages, then `result`, `canceled` or `error` |
| `cancel` | `jobId` (optional; omit to cancel whatever is running) | `{"type":"canceled","ok":true,"cancelRequested":true\|false,"jobId"?,"generation"?}` |
| `unload` | | `{"type":"unloaded"}` |
| `shutdown` | | `{"type":"bye"}`, then the process exits with code 0 |

Result:

```json
{"protocolVersion":1,"requestId":"…","jobId":"…","generation":4,"type":"result","ok":true,
 "result":{"text":"Hello world.","language":"en",
           "segments":[{"id":0,"start":0.0,"end":2.4,"text":"Hello world.","words":null}],
           "durationSeconds":2.6,"backend":"cpu"}}
```

With `wordTimestamps:true`, `words` is `[{"text","start","end","p"}]`. Word
times come from the engine's token-level timestamps, grouped into words at
leading spaces. `p` is the mean token probability.

A canceled job answers its own `requestId` with
`{"type":"canceled","ok":false,"jobId","generation","error":{"code":"canceled",…}}`.
The `cancel` command gets its own acknowledgement, described in the table
above. A cancel that arrives after recognition has finished still produces
`canceled`, so a result the core asked to drop is never delivered.

Error:

```json
{"protocolVersion":1,"requestId":"…","type":"error","ok":false,
 "error":{"code":"audio_read_failed","message":"…"},"jobId":"…","generation":4}
```

`jobId` and `generation` are included for `transcribe` errors. Error codes:

| Code | When |
| --- | --- |
| `bad_request` | Malformed JSON, oversized line, wrong `protocolVersion`, missing or invalid field, unknown command, unknown option, unknown language code |
| `model_load_failed` | Model file missing or unreadable, failed to load, or `backend:"metal"` requested without a usable GPU |
| `audio_read_failed` | Missing file, not RIFF/WAVE, unsupported encoding, sample rate other than 16000 Hz, or audio longer than `maxAudioSeconds` |
| `incompatible_model` | `task:"translate"`, or a non-English `language`, with an English-only model |
| `canceled` | Inside the `canceled` reply of a canceled job |
| `busy` | A `warm`, `transcribe` or `unload` arrived while another one was running. Requests are serialized by rejection, not queued |
| `unsupported_cpu` | The CPU lacks the compiled instruction level (see above) |
| `internal` | Engine failure or another unexpected error |

### Transcribe options to engine parameters

Unknown option keys are rejected with `bad_request`, never ignored.

| Option | Default | Engine (`whisper_full_params`) |
| --- | --- | --- |
| `task`: `transcribe`\|`translate` | `transcribe` | `translate` |
| `language`: code or `null`/`"auto"` | auto | `language` (English-only models always use `en`) |
| `initialPrompt` (≤ 16 KiB) | `""` | `initial_prompt` |
| `temperature` 0–1 | 0 | `temperature` (engine fallback increment 0.2 stays on) |
| `decoding`: `greedy`\|`beam` | `greedy` | `WHISPER_SAMPLING_GREEDY` / `_BEAM_SEARCH` |
| `beamSize` 1–8 | 5 | `beam_search.beam_size` |
| `bestOf` 1–8 | 5 | `greedy.best_of` |
| `wordTimestamps` | false | `token_timestamps` |
| `noContext` | true | `no_context` |
| `noSpeechThreshold` 0–1 | 0.6 | `no_speech_thold` |
| `logprobThreshold` −100–0 | −1.0 | `logprob_thold` |
| `threads` 0–64 | 0 means `min(hardware_concurrency, 8)` | `n_threads` |
| `maxSegmentLength` (chars, 0 means unlimited) | 0 | `max_len` + `split_on_word` (also turns on token timestamps) |

Not exposed, because the pinned engine has no equivalent semantics: clip
ranges and patience.

Whole-segment non-speech annotations from the engine (for example
`[BLANK_AUDIO]` or `(music)`) are dropped, so they are never inserted as
dictated text.

### Audio input

The core always passes a 16 kHz WAV. The reader parses RIFF/WAVE itself,
with no audio libraries. It skips unknown chunks (such as `LIST` and `FLLR`)
and accepts `WAVE_FORMAT_EXTENSIBLE`, 16-bit PCM and 32-bit float, mono or
stereo (averaged to mono). Any other sample rate gets `audio_read_failed`;
the worker never resamples.

### Threads and lifecycle

- The main thread reads stdin and answers `hello`, `capabilities`, `cancel`
  and `shutdown` immediately, even while recognition runs.
- A single engine thread owns the only model context. `warm` and `transcribe`
  load the requested model, unloading a different one first.
- Cancellation sets an atomic flag. The flag is wired to both
  `abort_callback` and `encoder_begin_callback`.
- `backend:"auto"` probes the Metal device on arm64 builds. If Metal cannot
  initialize, or the context fails to load with GPU on, the worker retries on
  CPU and reports the backend actually used.
- Exit: on `shutdown` (after `bye`), on stdin EOF (the parent closed the pipe
  or died), and on macOS/Linux when `getppid()` changes (checked every
  second). On Windows the parent's job object handles orphaning.

## Testing

```sh
scripts/build-worker.sh x86_64-apple-darwin
curl -L -o /tmp/ggml-tiny.en.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin
shasum -a 256 /tmp/ggml-tiny.en.bin   # must equal the manifest's sha256 for tiny.en
python3 -I speech-worker/tests/smoke.py \
  --worker src-tauri/binaries/sayvane-speech-x86_64-apple-darwin \
  --model /tmp/ggml-tiny.en.bin
```

The smoke test checks the protocol's structure. It covers hello and
capabilities, warm, recognition with word timestamps (hello.wav), silence,
`busy` rejection, cancel before and during recognition, translate refusal,
wrong sample rate, missing files, validation errors, malformed and oversized
lines, unload, shutdown and stdin EOF. Fixtures are in `tests/fixtures/`:
`hello.wav` is about 3.7 s of 16 kHz mono 16-bit speech made with macOS
`say` and `afconvert`, and `silence.wav` is 1 s of zeros.

### Verification status

- x86_64-apple-darwin: built and smoke-tested on an Intel Mac (macOS 15.7,
  Apple clang 12, macOS 11.0 SDK, CMake 4.4.5). All checks pass. The binary
  links only `libSystem`, `libc++` and Accelerate.
- aarch64-apple-darwin: cross-compiled on the same Intel Mac. The build
  succeeds and `lipo -archs` reports `arm64`, but it has **not been run**.
  Because it was built against the 11.0 SDK, newer Metal features that ggml
  gates on SDK availability are compiled out. Release builds must run on an
  Apple Silicon machine with a current Xcode, and run the smoke test there.
- x86_64-pc-windows-msvc: `build-worker.ps1` has **not been run yet**.
