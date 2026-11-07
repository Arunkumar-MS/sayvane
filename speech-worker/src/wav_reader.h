// Minimal, dependency-free RIFF/WAVE reader for the worker's audio input.
#pragma once

#include <cstdint>
#include <string>
#include <vector>

namespace sayvane {

struct WavReadResult {
    bool ok = false;
    std::string error;           // user-facing reason when !ok
    std::vector<float> samples;  // mono, [-1, 1]
    uint32_t sample_rate = 0;
    uint16_t channels = 0;
};

// Reads a 16 kHz WAV file. Accepts 16-bit PCM or 32-bit IEEE float, mono or
// stereo (stereo is averaged to mono). Unknown chunks are skipped.
// `max_seconds` bounds the decoded length.
WavReadResult read_wav_16k_mono(const std::string& path, double max_seconds);

// Same as above, from an in-memory buffer (used by the file variant and tests).
WavReadResult parse_wav_16k_mono(const uint8_t* data, size_t size, double max_seconds);

}  // namespace sayvane
