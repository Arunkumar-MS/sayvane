// sayvane-speech: Sayvane's bundled local speech recognition worker.
//
// Protocol: newline-delimited UTF-8 JSON. Commands arrive on stdin, responses
// leave on stdout, diagnostics go to stderr only. See speech-worker/README.md.
//
// Threads:
//   - main thread: reads stdin, answers light commands (hello, capabilities,
//     cancel, shutdown) immediately, hands heavy commands (warm, transcribe,
//     unload) to the engine thread. It keeps reading while recognition runs,
//     so `cancel` is handled promptly.
//   - engine thread: owns the single model context and runs one heavy job at
//     a time. A second heavy command while one is running is rejected `busy`.
//   - (macOS/Linux) parent watchdog: exits when the parent process disappears.

#include <atomic>
#include <condition_variable>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <mutex>
#include <optional>
#include <string>
#include <thread>

#include "cpu_check.h"
#include "engine.h"
#include "platform.h"
#include "protocol.h"
#include "wav_reader.h"

using namespace sayvane;

namespace {

enum class JobKind { Warm, Transcribe, Unload };

struct Job {
    JobKind kind = JobKind::Warm;
    std::string request_id;
    std::string job_id;      // transcribe only
    int64_t generation = 0;  // transcribe only
    std::string model_path;
    std::string audio_path;
    BackendPref backend = BackendPref::Auto;
    TranscribeOptions options;
};

class Worker {
public:
    Worker() : cpu_(check_cpu_support()) {}

    int run();

private:
    void handle_line(const std::string& line);
    void handle_message(const json& msg);
    void submit(Job job);
    void engine_loop();
    json run_job(const Job& job);
    json run_transcribe(const Job& job);
    void stop_engine_thread();
    json job_error(const Job& job, const std::string& code, const std::string& message) const;
    json hello(const std::string& request_id) const;
    json capabilities(const std::string& request_id) const;

    Output out_;
    Engine engine_;
    CpuSupport cpu_;

    std::mutex mu_;
    std::condition_variable cv_;
    std::optional<Job> pending_;
    bool busy_ = false;  // a job is pending or running
    bool stopping_ = false;
    std::string active_job_id_;  // transcribe job id while busy
    int64_t active_generation_ = 0;
    std::atomic<bool> cancel_{false};
    std::thread engine_thread_;
};

json Worker::job_error(const Job& job, const std::string& code, const std::string& message) const {
    json e = make_error(job.request_id, code, message);
    if (job.kind == JobKind::Transcribe) {
        e["jobId"] = job.job_id;
        e["generation"] = job.generation;
    }
    return e;
}

json Worker::hello(const std::string& request_id) const {
    json r = make_response(request_id, "hello");
    r["worker"] = "sayvane-speech";
    r["workerVersion"] = SAYVANE_WORKER_VERSION;
    r["engineVersion"] = SAYVANE_ENGINE_VERSION;
    r["arch"] = SAYVANE_ARCH_NAME;
    r["cpuLevel"] = cpu_.level;
    r["cpuSupported"] = cpu_.supported;
    if (!cpu_.supported) r["cpuMissingFeatures"] = cpu_.missing;
    return r;
}

json Worker::capabilities(const std::string& request_id) const {
    json r = make_response(request_id, "capabilities");
    json backends = json::array();
    if (cpu_.supported) {
        for (const auto& b : Engine::available_backends()) backends.push_back(b);
    }
    r["backends"] = backends;
    r["arch"] = SAYVANE_ARCH_NAME;
    r["engineVersion"] = SAYVANE_ENGINE_VERSION;
    r["cpuLevel"] = cpu_.level;
    r["cpuSupported"] = cpu_.supported;
    r["maxAudioSeconds"] = kMaxAudioSeconds;
    r["supportsTranslate"] = true;
    r["supportsWordTimestamps"] = true;
    r["supportsBeamSearch"] = true;
    r["maxBeamSize"] = kMaxBeamSize;
    r["maxLineBytes"] = kMaxLineBytes;
    r["options"] = json::array({"task", "language", "initialPrompt", "temperature", "decoding", "beamSize",
                                "bestOf", "wordTimestamps", "noContext", "noSpeechThreshold",
                                "logprobThreshold", "threads", "maxSegmentLength"});
    const LoadedModelInfo mi = engine_.info();
    if (mi.loaded) {
        r["loadedModel"] = {{"modelPath", mi.model_path},
                            {"backend", mi.backend},
                            {"multilingual", mi.multilingual},
                            {"canTranslate", mi.multilingual}};
    } else {
        r["loadedModel"] = nullptr;
    }
    return r;
}

void Worker::submit(Job job) {
    std::unique_lock<std::mutex> lock(mu_);
    if (stopping_) return;
    if (busy_) {
        json e = make_error(job.request_id, codes::kBusy,
                            "The speech engine is busy with another request; requests are processed one at a time.");
        if (job.kind == JobKind::Transcribe) {
            e["jobId"] = job.job_id;
            e["generation"] = job.generation;
        }
        lock.unlock();
        out_.send(e);
        return;
    }
    busy_ = true;
    cancel_.store(false);
    if (job.kind == JobKind::Transcribe) {
        active_job_id_ = job.job_id;
        active_generation_ = job.generation;
    } else {
        active_job_id_.clear();
    }
    pending_ = std::move(job);
    cv_.notify_all();
}

void Worker::engine_loop() {
    for (;;) {
        Job job;
        {
            std::unique_lock<std::mutex> lock(mu_);
            cv_.wait(lock, [&] { return stopping_ || pending_.has_value(); });
            if (!pending_.has_value()) return;  // stopping with nothing queued
            job = std::move(*pending_);
            pending_.reset();
        }
        json reply;
        try {
            reply = run_job(job);
        } catch (const std::exception& ex) {
            std::fprintf(stderr, "sayvane-speech: unexpected error: %s\n", ex.what());
            reply = job_error(job, codes::kInternal, "The speech engine hit an unexpected error.");
        } catch (...) {
            reply = job_error(job, codes::kInternal, "The speech engine hit an unexpected error.");
        }
        bool stop;
        {
            // Become idle BEFORE sending the terminal reply, so a client that
            // sends its next request as soon as it reads this one is not told `busy`.
            std::lock_guard<std::mutex> lock(mu_);
            busy_ = false;
            active_job_id_.clear();
            stop = stopping_;
        }
        out_.send(reply);
        if (stop) return;
    }
}

json Worker::run_job(const Job& job) {
    switch (job.kind) {
        case JobKind::Unload: {
            engine_.unload();
            return make_response(job.request_id, "unloaded");
        }
        case JobKind::Warm: {
            LoadOutcome lo = engine_.load(job.model_path, job.backend);
            if (!lo.ok) return job_error(job, lo.error_code, lo.error_message);
            const LoadedModelInfo mi = engine_.info();
            json r = make_response(job.request_id, "warmed");
            r["modelPath"] = mi.model_path;
            r["backend"] = mi.backend;
            r["multilingual"] = mi.multilingual;
            r["canTranslate"] = mi.multilingual;
            r["alreadyLoaded"] = lo.already_loaded;
            r["loadMs"] = lo.load_ms;
            return r;
        }
        case JobKind::Transcribe:
            return run_transcribe(job);
    }
    return job_error(job, codes::kInternal, "Unknown job kind.");
}

json Worker::run_transcribe(const Job& job) {
    auto canceled = [&]() {
        json r = make_response(job.request_id, "canceled", false);
        r["jobId"] = job.job_id;
        r["generation"] = job.generation;
        r["error"] = {{"code", codes::kCanceled}, {"message", "Recognition was canceled."}};
        return r;
    };

    LoadOutcome lo = engine_.load(job.model_path, job.backend);
    if (!lo.ok) return job_error(job, lo.error_code, lo.error_message);
    if (cancel_.load()) return canceled();

    WavReadResult wav = read_wav_16k_mono(job.audio_path, kMaxAudioSeconds);
    if (!wav.ok) return job_error(job, codes::kAudioReadFailed, wav.error);

    std::function<void(int)> on_progress = [&](int percent) {
        json p = make_response(job.request_id, "progress");
        p["jobId"] = job.job_id;
        p["generation"] = job.generation;
        p["percent"] = percent;
        out_.send(p);
    };

    TranscribeOutcome res = engine_.transcribe(wav.samples, job.options, &cancel_, on_progress);
    switch (res.status) {
        case TranscribeOutcome::Status::Canceled:
            return canceled();
        case TranscribeOutcome::Status::Error:
            return job_error(job, res.error_code, res.error_message);
        case TranscribeOutcome::Status::Ok:
            break;
    }

    json segments = json::array();
    for (const auto& s : res.segments) {
        json seg = {{"id", s.id}, {"start", s.start}, {"end", s.end}, {"text", s.text}};
        if (s.has_words) {
            json words = json::array();
            for (const auto& w : s.words) {
                words.push_back({{"text", w.text}, {"start", w.start}, {"end", w.end}, {"p", w.p}});
            }
            seg["words"] = words;
        } else {
            seg["words"] = nullptr;
        }
        segments.push_back(seg);
    }
    json r = make_response(job.request_id, "result");
    r["jobId"] = job.job_id;
    r["generation"] = job.generation;
    r["result"] = {{"text", res.text},
                   {"language", res.language.empty() ? json(nullptr) : json(res.language)},
                   {"segments", segments},
                   {"durationSeconds", res.duration_seconds},
                   {"backend", res.backend}};
    return r;
}

void Worker::handle_message(const json& msg) {
    // Best-effort request id for error replies, even when the envelope is invalid.
    json rid = nullptr;
    if (msg.is_object()) {
        auto it = msg.find("requestId");
        if (it != msg.end() && it->is_string() && it->get_ref<const std::string&>().size() <= kMaxIdLength) rid = *it;
    }
    try {
        const Envelope env = parse_envelope(msg);
        const std::string& cmd = env.command;

        if (cmd == "hello") {
            out_.send(hello(env.request_id));
            return;
        }
        if (cmd == "capabilities") {
            out_.send(capabilities(env.request_id));
            return;
        }
        if (cmd == "cancel") {
            std::string target;
            if (msg.contains("jobId") && !msg["jobId"].is_null()) target = require_id(msg, "jobId");
            bool requested = false;
            json r = make_response(env.request_id, "canceled");
            {
                std::lock_guard<std::mutex> lock(mu_);
                if (busy_ && !active_job_id_.empty() && (target.empty() || target == active_job_id_)) {
                    cancel_.store(true);
                    requested = true;
                    r["jobId"] = active_job_id_;
                    r["generation"] = active_generation_;
                } else if (!target.empty()) {
                    r["jobId"] = target;
                }
            }
            // Acknowledges the cancel request itself. The canceled job replies
            // separately with its own requestId, type "canceled" and ok:false.
            r["cancelRequested"] = requested;
            out_.send(r);
            return;
        }
        if (cmd == "shutdown") {
            stop_engine_thread();
            engine_.unload();
            out_.send(make_response(env.request_id, "bye"));
            std::fflush(stdout);
            std::exit(0);
        }

        Job job;
        job.request_id = env.request_id;
        if (cmd == "warm") {
            job.kind = JobKind::Warm;
            job.model_path = require_path(msg, "modelPath");
            job.backend = parse_backend(msg);
        } else if (cmd == "transcribe") {
            job.kind = JobKind::Transcribe;
            job.job_id = require_id(msg, "jobId");
            job.generation = require_generation(msg);
            rid = env.request_id;
            try {
                job.model_path = require_path(msg, "modelPath");
                job.audio_path = require_path(msg, "audioPath");
                job.backend = parse_backend(msg);
                job.options = parse_options(msg);
            } catch (const ProtocolError& pe) {
                out_.send(job_error(job, pe.code, pe.what()));
                return;
            }
        } else if (cmd == "unload") {
            job.kind = JobKind::Unload;
        } else {
            throw ProtocolError(codes::kBadRequest, "unknown command: " + cmd);
        }

        if (!cpu_.supported && job.kind != JobKind::Unload) {
            out_.send(job_error(job, codes::kUnsupportedCpu,
                                "This computer's processor lacks instructions required by the speech engine (" +
                                    cpu_.missing + ")."));
            return;
        }
        submit(std::move(job));
    } catch (const ProtocolError& pe) {
        out_.send(make_error(rid, pe.code, pe.what()));
    }
}

void Worker::handle_line(const std::string& line) {
    // Ignore blank lines.
    bool blank = true;
    for (char c : line) {
        if (c != ' ' && c != '\t' && c != '\r') {
            blank = false;
            break;
        }
    }
    if (blank) return;
    json msg;
    try {
        msg = json::parse(line);
    } catch (const json::exception&) {
        out_.send(make_error(nullptr, codes::kBadRequest, "The message is not valid UTF-8 JSON."));
        return;
    }
    handle_message(msg);
}

void Worker::stop_engine_thread() {
    {
        std::lock_guard<std::mutex> lock(mu_);
        stopping_ = true;
        cancel_.store(true);
        cv_.notify_all();
    }
    if (engine_thread_.joinable()) engine_thread_.join();
}

int Worker::run() {
    engine_thread_ = std::thread([this] { engine_loop(); });

    start_parent_watchdog([] {
        std::fprintf(stderr, "sayvane-speech: parent process exited; shutting down\n");
        std::fflush(stderr);
        std::_Exit(0);
    });

    // Bounded line reader: lines longer than kMaxLineBytes are discarded up to
    // the next newline and answered with bad_request.
    std::string line;
    line.reserve(64 * 1024);
    bool overflow = false;
    char buf[64 * 1024];
    for (;;) {
        if (!std::fgets(buf, sizeof(buf), stdin)) {
            // EOF (parent closed the pipe or died) or read error.
            if (!overflow && !line.empty()) handle_line(line);
            break;
        }
        size_t n = std::strlen(buf);
        const bool has_nl = n > 0 && buf[n - 1] == '\n';
        if (has_nl) --n;
        if (!overflow) {
            if (line.size() + n > kMaxLineBytes) {
                overflow = true;
                line.clear();
                line.shrink_to_fit();
            } else {
                line.append(buf, n);
            }
        }
        if (has_nl) {
            if (overflow) {
                out_.send(make_error(nullptr, codes::kBadRequest, "The message exceeds the 1 MiB line limit."));
            } else {
                if (!line.empty() && line.back() == '\r') line.pop_back();
                handle_line(line);
            }
            line.clear();
            overflow = false;
        }
    }

    stop_engine_thread();
    engine_.unload();
    return 0;
}

}  // namespace

int main() {
    set_stdio_binary();
    install_engine_logging();
    Worker worker;
    return worker.run();
}
