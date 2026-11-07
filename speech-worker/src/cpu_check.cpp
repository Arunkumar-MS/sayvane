// This translation unit is compiled WITHOUT the engine's SIMD flags, so it is
// safe to execute on any x86-64 CPU. The engine's ggml CPU code (compiled with
// -mavx2 etc. when SAYVANE_X86_LEVEL=avx2) is only reached after this check.
#include "cpu_check.h"

#if defined(__x86_64__) || defined(_M_X64) || defined(__i386__) || defined(_M_IX86)
#define SAYVANE_X86 1
#if defined(_MSC_VER)
#include <intrin.h>
#else
#include <cpuid.h>
#endif
#endif

#if defined(_WIN32) && defined(SAYVANE_REQUIRE_ARM_DOTPROD)
#include <windows.h>
#endif

namespace sayvane {

#ifdef SAYVANE_X86
static void cpuid(int leaf, int subleaf, unsigned regs[4]) {
#if defined(_MSC_VER)
    int r[4];
    __cpuidex(r, leaf, subleaf);
    for (int i = 0; i < 4; ++i) regs[i] = (unsigned)r[i];
#else
    __cpuid_count(leaf, subleaf, regs[0], regs[1], regs[2], regs[3]);
#endif
}

static unsigned long long read_xcr0() {
#if defined(_MSC_VER)
    return _xgetbv(0);
#else
    unsigned eax = 0, edx = 0;
    __asm__ volatile("xgetbv" : "=a"(eax), "=d"(edx) : "c"(0));
    return ((unsigned long long)edx << 32) | eax;
#endif
}
#endif

CpuSupport check_cpu_support() {
    CpuSupport out;
#ifndef SAYVANE_X86
    out.level = "arm64";
#if defined(_WIN32) && defined(SAYVANE_REQUIRE_ARM_DOTPROD)
    // 43 = PF_ARM_V82_DP_INSTRUCTIONS_AVAILABLE (Arm dot product, ARMv8.2).
    if (!IsProcessorFeaturePresent(43)) {
        out.supported = false;
        out.missing = "Arm dot product (ARMv8.2)";
    }
#endif
    return out;
#else
#if defined(SAYVANE_REQUIRE_AVX2)
    out.level = "avx2";
#elif defined(SAYVANE_REQUIRE_SSE42)
    out.level = "sse42";
#else
    out.level = "native";
#endif
    unsigned r[4] = {0, 0, 0, 0};
    cpuid(0, 0, r);
    const unsigned max_leaf = r[0];
    unsigned ecx1 = 0, ebx7 = 0;
    if (max_leaf >= 1) {
        cpuid(1, 0, r);
        ecx1 = r[2];
    }
    if (max_leaf >= 7) {
        cpuid(7, 0, r);
        ebx7 = r[1];
    }
    auto add_missing = [&](const char* name) {
        if (!out.missing.empty()) out.missing += ", ";
        out.missing += name;
        out.supported = false;
    };
    (void)ebx7;
#if defined(SAYVANE_REQUIRE_SSE42) || defined(SAYVANE_REQUIRE_AVX2)
    if (!(ecx1 & (1u << 20))) add_missing("SSE4.2");
#endif
#if defined(SAYVANE_REQUIRE_AVX2)
    const bool osxsave = (ecx1 & (1u << 27)) != 0;
    const bool avx = (ecx1 & (1u << 28)) != 0;
    const bool fma = (ecx1 & (1u << 12)) != 0;
    const bool f16c = (ecx1 & (1u << 29)) != 0;
    const bool avx2 = (ebx7 & (1u << 5)) != 0;
    const bool bmi2 = (ebx7 & (1u << 8)) != 0;
    bool os_ymm = false;
    if (osxsave) os_ymm = (read_xcr0() & 0x6) == 0x6;  // XMM + YMM state enabled by the OS
    if (!avx || !os_ymm) add_missing("AVX");
    if (!avx2) add_missing("AVX2");
    if (!fma) add_missing("FMA");
    if (!f16c) add_missing("F16C");
    if (!bmi2) add_missing("BMI2");
#else
    (void)read_xcr0;
#endif
    return out;
#endif
}

}  // namespace sayvane
