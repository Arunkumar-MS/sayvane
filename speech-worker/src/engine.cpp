// Speech engine wrapper. The engine is whisper.cpp v1.9.5 (MIT), statically
// linked. User-facing messages produced here use neutral wording only.
#include "engine.h"

#include <algorithm>
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <thread>

#include "ggml-backend.h"
#include "ggml.h"
#include "platform.h"
#include "whisper.h"

namespace sayvane {

namespace {

bool g_verbose = false;

void log_cb(enum ggml_log_level level, const char* text, void* /*user*/) {
    // Diagnostics go to stderr only. Without SAYVANE_SPEECH_VERBOSE, keep warnings and errors.
    if (!g_verbose && level != GGML_LOG_LEVEL_WARN && level != GGML_LOG_LEVEL_ERROR &&
        level != GGML_LOG_LEVEL_CONT) {
        return;
    }
    std::fputs(text, stderr);
    std::fflush(stderr);
}

// Model loader over a UTF-8 path (std::ifstream cannot open non-ASCII paths on Windows).
struct FileLoader {
    FILE* f = nullptr;
    bool eof = false;
};

size_t loader_read(void* ctx, void* out, size_t n) {
    auto* l = static_cast<FileLoader*>(ctx);
    size_t got = std::fread(out, 1, n, l->f);
    if (got < n) l->eof = true;
    return got;
}
bool loader_eof(void* ctx) {
    auto* l = static_cast<FileLoader*>(ctx);
    return l->eof || std::feof(l->f);
}
void loader_close(void* ctx) {
    auto* l = static_cast<FileLoader*>(ctx);
    if (l->f) std::fclose(l->f);
    l->f = nullptr;
}

whisper_context* init_from_path(const std::string& path, bool use_gpu) {
    FileLoader fl;
    fl.f = open_file_utf8(path, "rb");
    if (!fl.f) return nullptr;
    whisper_model_loader loader;
    loader.context = &fl;
    loader.read = loader_read;
    loader.eof = loader_eof;
    loader.close = loader_close;

    whisper_context_params cparams = whisper_context_default_params();
    cparams.use_gpu = use_gpu;
    cparams.gpu_device = 0;
    whisper_context* ctx = whisper_init_with_params(&loader, cparams);
    if (fl.f) std::fclose(fl.f);  // in case the engine did not call close()
    return ctx;
}

// Finds a GPU device in the engine's backend registry and checks that it can be
// initialized. The engine would silently fall back to CPU if this failed, so
// probing it ourselves lets us report the backend that is actually used.
#ifdef SAYVANE_HAS_METAL
bool gpu_backend_usable() {
    for (size_t i = 0; i < ggml_backend_dev_count(); ++i) {
        ggml_backend_dev_t dev = ggml_backend_dev_get(i);
        enum ggml_backend_dev_type t = ggml_backend_dev_type(dev);
        if (t == GGML_BACKEND_DEVICE_TYPE_GPU || t == GGML_BACKEND_DEVICE_TYPE_IGPU) {
            ggml_backend_t b = ggml_backend_dev_init(dev, nullptr);
            if (!b) return false;
            ggml_backend_free(b);
            return true;
        }
    }
    return false;
}
#endif

bool gpu_device_present() {
#ifdef SAYVANE_HAS_METAL
    for (size_t i = 0; i < ggml_backend_dev_count(); ++i) {
        enum ggml_backend_dev_type t = ggml_backend_dev_type(ggml_backend_dev_get(i));
        if (t == GGML_BACKEND_DEVICE_TYPE_GPU || t == GGML_BACKEND_DEVICE_TYPE_IGPU) return true;
    }
#endif
    return false;
}

std::string trim(const std::string& s) {
    size_t b = 0, e = s.size();
    while (b < e && (s[b] == ' ' || s[b] == '\t' || s[b] == '\n' || s[b] == '\r')) ++b;
    while (e > b && (s[e - 1] == ' ' || s[e - 1] == '\t' || s[e - 1] == '\n' || s[e - 1] == '\r')) --e;
    return s.substr(b, e - b);
}

// The engine marks non-speech with whole-segment annotations such as
// "[BLANK_AUDIO]" or "(music)". These must never be inserted as dictated text.
bool is_non_speech_annotation(const std::string& t) {
    if (t.size() < 2) return false;
    const char open = t.front(), close = t.back();
    if (!((open == '[' && close == ']') || (open == '(' && close == ')'))) return false;
    // Only a single bracketed group (no nested closing bracket before the end).
    return t.find(close) == t.size() - 1;
}

struct CallbackState {
    std::atomic<bool>* cancel = nullptr;
    const std::function<void(int)>* on_progress = nullptr;
    int last_percent = -1;
};

bool abort_cb(void* user) {
    auto* st = static_cast<CallbackState*>(user);
    return st->cancel && st->cancel->load(std::memory_order_relaxed);
}

bool encoder_begin_cb(whisper_context*, whisper_state*, void* user) {
    return !abort_cb(user);
}

void progress_cb(whisper_context*, whisper_state*, int progress, void* user) {
    auto* st = static_cast<CallbackState*>(user);
    int p = std::max(0, std::min(100, progress));
    if (p != st->last_percent) {
        st->last_percent = p;
        if (st->on_progress && *st->on_progress) (*st->on_progress)(p);
    }
}

int auto_threads() {
    unsigned hc = std::thread::hardware_concurrency();
    if (hc == 0) hc = 4;
    return (int)std::min<unsigned>(hc, 8);
}

}  // namespace

void install_engine_logging() {
    const char* v = std::getenv("SAYVANE_SPEECH_VERBOSE");
    g_verbose = v && *v && std::strcmp(v, "0") != 0;
    whisper_log_set(log_cb, nullptr);
    ggml_log_set(log_cb, nullptr);
}

Engine::Engine() = default;

Engine::~Engine() { unload(); }

std::vector<std::string> Engine::available_backends() {
    std::vector<std::string> out;
    if (gpu_device_present()) out.push_back("metal");
    out.push_back("cpu");
    return out;
}

LoadedModelInfo Engine::info() const {
    std::lock_guard<std::mutex> lock(info_mu_);
    return info_;
}

void Engine::unload() {
    if (ctx_) {
        whisper_free(ctx_);
        ctx_ = nullptr;
    }
    std::lock_guard<std::mutex> lock(info_mu_);
    info_ = LoadedModelInfo();
}

LoadOutcome Engine::load(const std::string& model_path, BackendPref pref) {
    LoadOutcome out;
    {
        std::lock_guard<std::mutex> lock(info_mu_);
        if (ctx_ && info_.loaded && info_.model_path == model_path && loaded_pref_ == pref) {
            out.ok = true;
            out.already_loaded = true;
            return out;
        }
    }
    // One model at a time: release the previous one before loading another.
    unload();

    unsigned long long size = 0;
    if (!regular_file_size(model_path, &size) || size < 1024) {
        out.error_code = codes::kModelLoadFailed;
        out.error_message = "The speech model file is missing or unreadable.";
        return out;
    }

    const auto t0 = std::chrono::steady_clock::now();
    bool use_gpu = false;
    if (pref == BackendPref::Metal || pref == BackendPref::Auto) {
#ifdef SAYVANE_HAS_METAL
        use_gpu = gpu_backend_usable();
#endif
        if (pref == BackendPref::Metal && !use_gpu) {
            out.error_code = codes::kModelLoadFailed;
            out.error_message = "GPU acceleration is not available on this device.";
            return out;
        }
    }

    whisper_context* ctx = init_from_path(model_path, use_gpu);
    if (!ctx && use_gpu && pref == BackendPref::Auto) {
        std::fprintf(stderr, "sayvane-speech: GPU initialization failed, retrying on CPU\n");
        use_gpu = false;
        ctx = init_from_path(model_path, false);
    }
    if (!ctx) {
        out.error_code = codes::kModelLoadFailed;
        out.error_message = "The speech model could not be loaded. The file may be damaged or in an unsupported format.";
        return out;
    }
    ctx_ = ctx;
    loaded_pref_ = pref;
    {
        std::lock_guard<std::mutex> lock(info_mu_);
        info_.loaded = true;
        info_.model_path = model_path;
        info_.backend = use_gpu ? "metal" : "cpu";
        info_.multilingual = whisper_is_multilingual(ctx_) != 0;
    }
    out.ok = true;
    out.load_ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - t0).count();
    return out;
}

TranscribeOutcome Engine::transcribe(const std::vector<float>& samples, const TranscribeOptions& opts,
                                     std::atomic<bool>* cancel, const std::function<void(int)>& on_progress) {
    TranscribeOutcome out;
    if (!ctx_) {
        out.error_code = codes::kInternal;
        out.error_message = "No speech model is loaded.";
        return out;
    }
    const LoadedModelInfo mi = info();
    out.backend = mi.backend;
    out.duration_seconds = (double)samples.size() / 16000.0;

    // Capability checks: never silently treat a request as something else.
    if (opts.translate && !mi.multilingual) {
        out.error_code = codes::kIncompatibleModel;
        out.error_message = "The selected model is English-only and cannot translate. Choose a multilingual model.";
        return out;
    }
    std::string language = opts.language;
    if (!language.empty()) {
        if (whisper_lang_id(language.c_str()) < 0) {
            out.error_code = codes::kBadRequest;
            out.error_message = "Unknown language code: " + language;
            return out;
        }
        if (!mi.multilingual && language != "en" && language != "english") {
            out.error_code = codes::kIncompatibleModel;
            out.error_message = "The selected model is English-only and cannot recognize this language.";
            return out;
        }
    } else if (!mi.multilingual) {
        language = "en";
    }

    if (cancel && cancel->load()) {
        out.status = TranscribeOutcome::Status::Canceled;
        return out;
    }

    // Less than 100 ms of audio: nothing to recognize (the engine would skip it too).
    if (samples.size() < 1600) {
        out.status = TranscribeOutcome::Status::Ok;
        out.language = language;
        return out;
    }

    whisper_full_params wp = whisper_full_default_params(opts.beam ? WHISPER_SAMPLING_BEAM_SEARCH : WHISPER_SAMPLING_GREEDY);
    wp.n_threads = opts.threads > 0 ? opts.threads : auto_threads();
    wp.translate = opts.translate;
    wp.no_context = opts.no_context;
    wp.print_special = false;
    wp.print_progress = false;
    wp.print_realtime = false;
    wp.print_timestamps = false;
    wp.language = language.empty() ? "auto" : language.c_str();
    wp.detect_language = false;
    wp.initial_prompt = opts.initial_prompt.empty() ? nullptr : opts.initial_prompt.c_str();
    wp.temperature = opts.temperature;
    wp.no_speech_thold = opts.no_speech_threshold;
    wp.logprob_thold = opts.logprob_threshold;
    wp.greedy.best_of = opts.best_of;
    wp.beam_search.beam_size = opts.beam_size;
    wp.token_timestamps = opts.word_timestamps || opts.max_segment_length > 0;
    if (opts.max_segment_length > 0) {
        wp.max_len = opts.max_segment_length;
        wp.split_on_word = true;
    }

    CallbackState cbs;
    cbs.cancel = cancel;
    cbs.on_progress = &on_progress;
    wp.abort_callback = abort_cb;
    wp.abort_callback_user_data = &cbs;
    wp.encoder_begin_callback = encoder_begin_cb;
    wp.encoder_begin_callback_user_data = &cbs;
    wp.progress_callback = progress_cb;
    wp.progress_callback_user_data = &cbs;

    const int rc = whisper_full(ctx_, wp, samples.data(), (int)samples.size());

    // A cancel that arrives after completion still wins: the caller asked not
    // to use this result, and must not insert it.
    if (cancel && cancel->load()) {
        out.status = TranscribeOutcome::Status::Canceled;
        return out;
    }
    if (rc != 0) {
        out.error_code = codes::kInternal;
        out.error_message = "Speech recognition failed (engine status " + std::to_string(rc) + ").";
        return out;
    }

    const int lang_id = whisper_full_lang_id(ctx_);
    const char* lang_str = lang_id >= 0 ? whisper_lang_str(lang_id) : nullptr;
    out.language = lang_str ? lang_str : language;

    const whisper_token eot = whisper_token_eot(ctx_);
    const int n_seg = whisper_full_n_segments(ctx_);
    std::string full;
    for (int i = 0; i < n_seg; ++i) {
        Segment seg;
        seg.id = i;
        seg.start = (double)whisper_full_get_segment_t0(ctx_, i) / 100.0;
        seg.end = (double)whisper_full_get_segment_t1(ctx_, i) / 100.0;
        const char* t = whisper_full_get_segment_text(ctx_, i);
        const std::string raw = t ? t : "";
        seg.text = trim(raw);
        if (is_non_speech_annotation(seg.text)) continue;
        seg.id = (int)out.segments.size();
        if (!seg.text.empty()) {
            if (!full.empty()) full.push_back(' ');
            full += seg.text;
        }
        if (opts.word_timestamps) {
            seg.has_words = true;
            const int n_tok = whisper_full_n_tokens(ctx_, i);
            float p_sum = 0;
            int p_count = 0;
            for (int j = 0; j < n_tok; ++j) {
                const whisper_token_data td = whisper_full_get_token_data(ctx_, i, j);
                if (td.id >= eot) continue;  // special and timestamp tokens
                const char* tt = whisper_full_get_token_text(ctx_, i, j);
                std::string tok = tt ? tt : "";
                if (tok.empty()) continue;
                const bool starts_word = seg.words.empty() || tok[0] == ' ';
                if (starts_word) {
                    if (!seg.words.empty() && p_count > 0) seg.words.back().p = p_sum / (float)p_count;
                    Word w;
                    w.text = trim(tok);
                    w.start = (double)td.t0 / 100.0;
                    w.end = (double)td.t1 / 100.0;
                    seg.words.push_back(w);
                    p_sum = td.p;
                    p_count = 1;
                } else {
                    Word& w = seg.words.back();
                    w.text += tok;
                    w.end = (double)td.t1 / 100.0;
                    p_sum += td.p;
                    ++p_count;
                }
            }
            if (!seg.words.empty() && p_count > 0) seg.words.back().p = p_sum / (float)p_count;
            // Drop words that are only whitespace after trimming.
            seg.words.erase(std::remove_if(seg.words.begin(), seg.words.end(),
                                           [](const Word& w) { return w.text.empty(); }),
                            seg.words.end());
        }
        out.segments.push_back(std::move(seg));
    }
    out.text = full;
    out.status = TranscribeOutcome::Status::Ok;
    return out;
}

}  // namespace sayvane
