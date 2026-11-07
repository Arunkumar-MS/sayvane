#include "protocol.h"

#include <cmath>
#include <cstdio>
#include <set>

namespace sayvane {

namespace {

[[noreturn]] void bad(const std::string& msg) { throw ProtocolError(codes::kBadRequest, msg); }

bool is_integer(const json& v) { return v.is_number_integer() || v.is_number_unsigned(); }

float get_float(const json& opts, const char* key, float def, float lo, float hi) {
    auto it = opts.find(key);
    if (it == opts.end() || it->is_null()) return def;
    if (!it->is_number()) bad(std::string("options.") + key + " must be a number");
    double v = it->get<double>();
    if (!std::isfinite(v) || v < lo || v > hi) {
        bad(std::string("options.") + key + " is out of range");
    }
    return (float)v;
}

int get_int(const json& opts, const char* key, int def, int lo, int hi) {
    auto it = opts.find(key);
    if (it == opts.end() || it->is_null()) return def;
    if (!is_integer(*it)) bad(std::string("options.") + key + " must be an integer");
    if (it->is_number_unsigned()) {
        uint64_t u = it->get<uint64_t>();
        if (u > (uint64_t)hi) bad(std::string("options.") + key + " is out of range");
        return (int)u;
    }
    int64_t v = it->get<int64_t>();
    if (v < lo || v > hi) bad(std::string("options.") + key + " is out of range");
    return (int)v;
}

bool get_bool(const json& opts, const char* key, bool def) {
    auto it = opts.find(key);
    if (it == opts.end() || it->is_null()) return def;
    if (!it->is_boolean()) bad(std::string("options.") + key + " must be a boolean");
    return it->get<bool>();
}

}  // namespace

Envelope parse_envelope(const json& msg) {
    if (!msg.is_object()) bad("message must be a JSON object");
    auto pv = msg.find("protocolVersion");
    if (pv == msg.end() || !is_integer(*pv)) bad("protocolVersion is required");
    if (pv->get<int64_t>() != kProtocolVersion) {
        bad("unsupported protocolVersion " + pv->dump() + "; this worker speaks version 1");
    }
    Envelope env;
    env.request_id = require_id(msg, "requestId");
    auto cmd = msg.find("command");
    if (cmd == msg.end() || !cmd->is_string()) bad("command is required");
    env.command = cmd->get<std::string>();
    return env;
}

std::string require_id(const json& msg, const char* field) {
    auto it = msg.find(field);
    if (it == msg.end() || !it->is_string()) bad(std::string(field) + " must be a string");
    std::string s = it->get<std::string>();
    if (s.empty() || s.size() > kMaxIdLength) bad(std::string(field) + " must be 1-256 bytes");
    for (unsigned char c : s) {
        if (c < 0x20) bad(std::string(field) + " contains control characters");
    }
    return s;
}

int64_t require_generation(const json& msg) {
    auto it = msg.find("generation");
    if (it == msg.end() || !is_integer(*it)) bad("generation must be a non-negative integer");
    if (it->is_number_unsigned()) {
        uint64_t u = it->get<uint64_t>();
        if (u > (uint64_t)INT64_MAX) bad("generation is out of range");
        return (int64_t)u;
    }
    int64_t g = it->get<int64_t>();
    if (g < 0) bad("generation must be a non-negative integer");
    return g;
}

std::string require_path(const json& msg, const char* field) {
    auto it = msg.find(field);
    if (it == msg.end() || !it->is_string()) bad(std::string(field) + " must be a string");
    std::string s = it->get<std::string>();
    if (s.empty() || s.size() > kMaxPathBytes) bad(std::string(field) + " has an invalid length");
    if (s.find('\0') != std::string::npos) bad(std::string(field) + " contains a NUL byte");
    return s;
}

BackendPref parse_backend(const json& msg) {
    auto it = msg.find("backend");
    if (it == msg.end() || it->is_null()) return BackendPref::Auto;
    if (!it->is_string()) bad("backend must be a string");
    const std::string b = it->get<std::string>();
    if (b == "auto") return BackendPref::Auto;
    if (b == "cpu") return BackendPref::Cpu;
    if (b == "metal" || b == "gpu") return BackendPref::Metal;
    bad("backend must be one of auto, cpu, metal");
}

const char* backend_name(BackendPref b) {
    switch (b) {
        case BackendPref::Cpu: return "cpu";
        case BackendPref::Metal: return "metal";
        default: return "auto";
    }
}

TranscribeOptions parse_options(const json& msg) {
    TranscribeOptions o;
    auto it = msg.find("options");
    if (it == msg.end() || it->is_null()) return o;
    if (!it->is_object()) bad("options must be an object");
    const json& opts = *it;

    // Unsupported options are rejected, never silently ignored.
    static const std::set<std::string> known = {
        "task", "language", "initialPrompt", "temperature", "decoding", "beamSize", "bestOf",
        "wordTimestamps", "noContext", "noSpeechThreshold", "logprobThreshold", "threads",
        "maxSegmentLength"};
    for (auto el = opts.begin(); el != opts.end(); ++el) {
        if (!known.count(el.key())) bad("unsupported option: " + el.key());
    }

    if (auto t = opts.find("task"); t != opts.end() && !t->is_null()) {
        if (!t->is_string()) bad("options.task must be a string");
        const std::string task = t->get<std::string>();
        if (task == "transcribe") o.translate = false;
        else if (task == "translate") o.translate = true;
        else bad("options.task must be transcribe or translate");
    }
    if (auto l = opts.find("language"); l != opts.end() && !l->is_null()) {
        if (!l->is_string()) bad("options.language must be a string or null");
        std::string lang = l->get<std::string>();
        if (lang.size() > 32) bad("options.language is too long");
        if (lang != "auto") o.language = lang;  // validated against the engine later
    }
    if (auto p = opts.find("initialPrompt"); p != opts.end() && !p->is_null()) {
        if (!p->is_string()) bad("options.initialPrompt must be a string");
        o.initial_prompt = p->get<std::string>();
        if (o.initial_prompt.size() > kMaxPromptBytes) bad("options.initialPrompt is too long");
    }
    o.temperature = get_float(opts, "temperature", 0.0f, 0.0f, 1.0f);
    if (auto d = opts.find("decoding"); d != opts.end() && !d->is_null()) {
        if (!d->is_string()) bad("options.decoding must be a string");
        const std::string dec = d->get<std::string>();
        if (dec == "greedy") o.beam = false;
        else if (dec == "beam") o.beam = true;
        else bad("options.decoding must be greedy or beam");
    }
    o.beam_size = get_int(opts, "beamSize", 5, 1, kMaxBeamSize);
    o.best_of = get_int(opts, "bestOf", 5, 1, kMaxBestOf);
    o.word_timestamps = get_bool(opts, "wordTimestamps", false);
    o.no_context = get_bool(opts, "noContext", true);
    o.no_speech_threshold = get_float(opts, "noSpeechThreshold", 0.6f, 0.0f, 1.0f);
    o.logprob_threshold = get_float(opts, "logprobThreshold", -1.0f, -100.0f, 0.0f);
    o.threads = get_int(opts, "threads", 0, 0, kMaxThreads);
    o.max_segment_length = get_int(opts, "maxSegmentLength", 0, 0, 10000);
    return o;
}

json make_response(const std::string& request_id, const char* type, bool ok) {
    json r = json::object();
    r["protocolVersion"] = kProtocolVersion;
    r["requestId"] = request_id;
    r["type"] = type;
    r["ok"] = ok;
    return r;
}

json make_error(const json& request_id, const std::string& code, const std::string& message) {
    json r = json::object();
    r["protocolVersion"] = kProtocolVersion;
    r["requestId"] = request_id;
    r["type"] = "error";
    r["ok"] = false;
    r["error"] = {{"code", code}, {"message", message}};
    return r;
}

void Output::send(const json& msg) {
    // Engine text may contain partial UTF-8 sequences; replace instead of throwing.
    std::string line = msg.dump(-1, ' ', false, json::error_handler_t::replace);
    line.push_back('\n');
    std::lock_guard<std::mutex> lock(mu_);
    std::fwrite(line.data(), 1, line.size(), stdout);
    std::fflush(stdout);
}

}  // namespace sayvane
