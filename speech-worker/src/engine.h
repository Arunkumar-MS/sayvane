// Wrapper around the local speech engine (whisper.cpp). Owns exactly one model
// context. Not thread-safe for concurrent recognition: the worker serializes
// load/unload/transcribe on a single engine thread. info() may be called from
// any thread.
#pragma once

#include <atomic>
#include <functional>
#include <mutex>
#include <string>
#include <vector>

#include "protocol.h"

struct whisper_context;

namespace sayvane {

struct LoadedModelInfo {
    bool loaded = false;
    std::string model_path;
    std::string backend;  // "metal" | "cpu"
    bool multilingual = false;
};

struct LoadOutcome {
    bool ok = false;
    bool already_loaded = false;
    std::string error_code;
    std::string error_message;
    double load_ms = 0.0;
};

struct Word {
    std::string text;
    double start = 0, end = 0;
    float p = 0;
};

struct Segment {
    int id = 0;
    double start = 0, end = 0;
    std::string text;
    bool has_words = false;
    std::vector<Word> words;
};

struct TranscribeOutcome {
    enum class Status { Ok, Canceled, Error } status = Status::Error;
    std::string error_code;
    std::string error_message;
    std::string text;
    std::string language;
    std::vector<Segment> segments;
    double duration_seconds = 0;
    std::string backend;
};

class Engine {
public:
    Engine();
    ~Engine();

    // Backends compiled in and usable on this machine ("metal" listed first).
    static std::vector<std::string> available_backends();

    LoadOutcome load(const std::string& model_path, BackendPref pref);
    void unload();
    LoadedModelInfo info() const;

    TranscribeOutcome transcribe(const std::vector<float>& samples, const TranscribeOptions& opts,
                                 std::atomic<bool>* cancel, const std::function<void(int)>& on_progress);

private:
    whisper_context* ctx_ = nullptr;
    mutable std::mutex info_mu_;
    LoadedModelInfo info_;
    BackendPref loaded_pref_ = BackendPref::Auto;
};

void install_engine_logging();

}  // namespace sayvane
