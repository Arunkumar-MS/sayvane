// Speech worker protocol: newline-delimited UTF-8 JSON over stdin/stdout.
// See plan.md "Speech worker protocol" and speech-worker/README.md.
#pragma once

#include <cstdint>
#include <mutex>
#include <stdexcept>
#include <string>

#include "nlohmann/json.hpp"

namespace sayvane {

using json = nlohmann::json;

constexpr int kProtocolVersion = 1;
constexpr size_t kMaxLineBytes = 1024 * 1024;  // 1 MiB
constexpr double kMaxAudioSeconds = 2.0 * 60.0 * 60.0;  // 2 hours
constexpr size_t kMaxIdLength = 256;
constexpr size_t kMaxPathBytes = 32 * 1024;
constexpr size_t kMaxPromptBytes = 16 * 1024;
constexpr int kMaxBeamSize = 8;
constexpr int kMaxBestOf = 8;
constexpr int kMaxThreads = 64;

// Error codes (stable, machine readable).
namespace codes {
constexpr const char* kModelLoadFailed = "model_load_failed";
constexpr const char* kAudioReadFailed = "audio_read_failed";
constexpr const char* kBadRequest = "bad_request";
constexpr const char* kIncompatibleModel = "incompatible_model";
constexpr const char* kCanceled = "canceled";
constexpr const char* kInternal = "internal";
constexpr const char* kBusy = "busy";
constexpr const char* kUnsupportedCpu = "unsupported_cpu";
}  // namespace codes

struct ProtocolError : public std::runtime_error {
    std::string code;
    ProtocolError(std::string c, const std::string& msg) : std::runtime_error(msg), code(std::move(c)) {}
};

enum class BackendPref { Auto, Cpu, Metal };

struct TranscribeOptions {
    bool translate = false;
    std::string language;        // empty = auto detect
    std::string initial_prompt;
    float temperature = 0.0f;
    bool beam = false;
    int beam_size = 5;
    int best_of = 5;             // engine default
    bool word_timestamps = false;
    bool no_context = true;
    float no_speech_threshold = 0.6f;
    float logprob_threshold = -1.0f;
    int threads = 0;             // 0 = auto
    int max_segment_length = 0;  // 0 = unlimited (characters)
};

// Request envelope shared by every command.
struct Envelope {
    std::string request_id;
    std::string command;
};

Envelope parse_envelope(const json& msg);  // throws ProtocolError
BackendPref parse_backend(const json& msg);  // throws ProtocolError
std::string require_path(const json& msg, const char* field);  // throws ProtocolError
std::string require_id(const json& msg, const char* field);  // throws ProtocolError
int64_t require_generation(const json& msg);  // throws ProtocolError
TranscribeOptions parse_options(const json& msg);  // throws ProtocolError

const char* backend_name(BackendPref b);

// Base response object: {"protocolVersion":1,"requestId":...,"type":...,"ok":...}
json make_response(const std::string& request_id, const char* type, bool ok = true);
json make_error(const json& request_id, const std::string& code, const std::string& message);

// Thread-safe line writer for stdout. Each message is one line, flushed.
class Output {
public:
    void send(const json& msg);

private:
    std::mutex mu_;
};

}  // namespace sayvane
