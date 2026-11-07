// Startup check that the CPU supports the instruction level the engine was
// compiled for, so an unsupported CPU gets a clean protocol error instead of
// an illegal-instruction crash.
#pragma once

#include <string>

namespace sayvane {

struct CpuSupport {
    bool supported = true;
    std::string level;     // "sse42", "avx2", "native" or "arm64"
    std::string missing;   // human readable list of missing features
};

CpuSupport check_cpu_support();

}  // namespace sayvane
