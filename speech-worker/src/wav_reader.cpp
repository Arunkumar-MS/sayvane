#include "wav_reader.h"

#include <cstdio>
#include <cstring>

#include "platform.h"

namespace sayvane {

namespace {

constexpr uint32_t kRequiredRate = 16000;
constexpr uint16_t kFormatPcm = 0x0001;
constexpr uint16_t kFormatFloat = 0x0003;
constexpr uint16_t kFormatExtensible = 0xFFFE;

uint16_t rd16(const uint8_t* p) { return (uint16_t)(p[0] | (p[1] << 8)); }
uint32_t rd32(const uint8_t* p) {
    return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

WavReadResult fail(const std::string& msg) {
    WavReadResult r;
    r.ok = false;
    r.error = msg;
    return r;
}

}  // namespace

WavReadResult parse_wav_16k_mono(const uint8_t* data, size_t size, double max_seconds) {
    if (size < 12 || std::memcmp(data, "RIFF", 4) != 0 || std::memcmp(data + 8, "WAVE", 4) != 0) {
        return fail("The audio file is not a RIFF/WAVE file.");
    }

    bool have_fmt = false;
    uint16_t format = 0, channels = 0, bits = 0, block_align = 0;
    uint32_t rate = 0;
    const uint8_t* pcm = nullptr;
    size_t pcm_size = 0;

    size_t pos = 12;
    while (pos + 8 <= size) {
        const uint8_t* hdr = data + pos;
        const uint32_t chunk_size = rd32(hdr + 4);
        const size_t body = pos + 8;
        size_t avail = size - body;
        if (std::memcmp(hdr, "fmt ", 4) == 0) {
            if (chunk_size < 16 || chunk_size > avail) {
                return fail("The audio file has a malformed format chunk.");
            }
            const uint8_t* f = data + body;
            format = rd16(f);
            channels = rd16(f + 2);
            rate = rd32(f + 4);
            block_align = rd16(f + 12);
            bits = rd16(f + 14);
            if (format == kFormatExtensible) {
                if (chunk_size < 40) return fail("The audio file has a malformed extensible format chunk.");
                // SubFormat GUID: first two bytes carry the actual format tag.
                format = rd16(f + 24);
            }
            have_fmt = true;
        } else if (std::memcmp(hdr, "data", 4) == 0) {
            // Streaming writers may leave the size as 0 or 0xFFFFFFFF; clamp to
            // what is actually present.
            size_t n = chunk_size;
            if (n == 0 || n > avail) n = avail;
            pcm = data + body;
            pcm_size = n;
            break;  // data is the last chunk we care about
        }
        // Skip this chunk (chunks are padded to an even size).
        size_t advance = 8 + (size_t)chunk_size + (chunk_size & 1u);
        if ((size_t)chunk_size > avail) break;
        pos += advance;
    }

    if (!have_fmt) return fail("The audio file has no format chunk.");
    if (!pcm) return fail("The audio file has no audio data chunk.");
    if (channels < 1 || channels > 2) {
        return fail("Unsupported channel count " + std::to_string(channels) + "; expected mono or stereo.");
    }
    if (rate != kRequiredRate) {
        return fail("Unsupported sample rate " + std::to_string(rate) +
                    " Hz; the speech engine requires 16000 Hz audio.");
    }

    size_t bytes_per_sample = 0;
    if (format == kFormatPcm && bits == 16) {
        bytes_per_sample = 2;
    } else if (format == kFormatFloat && bits == 32) {
        bytes_per_sample = 4;
    } else {
        return fail("Unsupported audio encoding (format " + std::to_string(format) + ", " +
                    std::to_string(bits) + "-bit); expected 16-bit PCM or 32-bit float.");
    }
    const size_t frame_bytes = bytes_per_sample * channels;
    if (block_align != 0 && block_align != frame_bytes) {
        return fail("The audio file has an inconsistent block alignment.");
    }

    const size_t n_frames = pcm_size / frame_bytes;
    const double seconds = (double)n_frames / (double)kRequiredRate;
    if (max_seconds > 0 && seconds > max_seconds) {
        return fail("The audio is too long (" + std::to_string((long long)seconds) + " s; limit " +
                    std::to_string((long long)max_seconds) + " s).");
    }

    WavReadResult r;
    r.sample_rate = rate;
    r.channels = channels;
    r.samples.resize(n_frames);
    for (size_t i = 0; i < n_frames; ++i) {
        const uint8_t* fr = pcm + i * frame_bytes;
        float acc = 0.0f;
        for (uint16_t c = 0; c < channels; ++c) {
            const uint8_t* s = fr + c * bytes_per_sample;
            float v;
            if (bytes_per_sample == 2) {
                v = (float)(int16_t)rd16(s) / 32768.0f;
            } else {
                uint32_t u = rd32(s);
                std::memcpy(&v, &u, sizeof(v));
                if (!(v == v)) v = 0.0f;  // NaN
                if (v > 1.0f) v = 1.0f;
                if (v < -1.0f) v = -1.0f;
            }
            acc += v;
        }
        r.samples[i] = acc / (float)channels;
    }
    r.ok = true;
    return r;
}

WavReadResult read_wav_16k_mono(const std::string& path, double max_seconds) {
    unsigned long long file_size = 0;
    if (!regular_file_size(path, &file_size)) {
        return fail("The audio file does not exist or is not a regular file.");
    }
    // Upper bound: stereo float at the length limit plus generous header slack.
    const double limit_bytes = (max_seconds > 0 ? max_seconds : 0) * kRequiredRate * 8.0 + 16.0 * 1024 * 1024;
    if (max_seconds > 0 && (double)file_size > limit_bytes) {
        return fail("The audio file is too large.");
    }
    FILE* f = open_file_utf8(path, "rb");
    if (!f) return fail("The audio file could not be opened.");
    std::vector<uint8_t> buf((size_t)file_size);
    size_t got = buf.empty() ? 0 : std::fread(buf.data(), 1, buf.size(), f);
    std::fclose(f);
    if (got != buf.size()) return fail("The audio file could not be read completely.");
    return parse_wav_16k_mono(buf.data(), buf.size(), max_seconds);
}

}  // namespace sayvane
