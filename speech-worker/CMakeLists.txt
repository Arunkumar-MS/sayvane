# Sayvane local speech worker (sayvane-speech).
#
# Internally this links whisper.cpp (MIT, https://github.com/ggml-org/whisper.cpp)
# and its ggml tensor library statically. The product never brands the engine;
# see NOTICE.md for third-party licenses.

cmake_minimum_required(VERSION 3.21)

# Must be set before project() so it applies to every target, including the
# fetched engine. Ignored on non-Apple platforms.
if(NOT DEFINED CMAKE_OSX_DEPLOYMENT_TARGET)
    set(CMAKE_OSX_DEPLOYMENT_TARGET "13.0" CACHE STRING "Minimum macOS version")
endif()

# MSVC: static C/C++ runtime (/MT, /MTd) so no VC++ redistributable is needed.
cmake_policy(SET CMP0091 NEW)
# The engine's top-level CMakeLists declares an old minimum version; make sure
# it honours CMAKE_MSVC_RUNTIME_LIBRARY too.
set(CMAKE_POLICY_DEFAULT_CMP0091 NEW)
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded$<$<CONFIG:Debug>:Debug>" CACHE STRING "" FORCE)

project(sayvane_speech VERSION 0.1.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
    set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

include(FetchContent)

# ---------------------------------------------------------------------------
# Pinned engine: whisper.cpp v1.9.5 (GitHub tag tarball).
# ---------------------------------------------------------------------------
set(SAYVANE_ENGINE_VERSION "1.9.5")
set(SAYVANE_ENGINE_URL
    "https://github.com/ggml-org/whisper.cpp/archive/refs/tags/v${SAYVANE_ENGINE_VERSION}.tar.gz")
set(SAYVANE_ENGINE_SHA256
    "ff1a9053feb509ff9d7729703355541ae9690073a6b1c40eb692c962e0dc1720")

# Allow an offline/local copy of the exact same tarball (hash still enforced).
set(SAYVANE_ENGINE_TARBALL "" CACHE FILEPATH
    "Optional local path to whisper.cpp-v1.9.5.tar.gz (hash is still verified)")
if(SAYVANE_ENGINE_TARBALL)
    set(SAYVANE_ENGINE_URL "${SAYVANE_ENGINE_TARBALL}")
endif()

# ---------------------------------------------------------------------------
# CPU instruction policy for distributable x86-64 builds (see README.md).
#   sse42 : SSE4.2 only. Safe on practically every x86-64 PC that runs Windows 10.
#   avx2  : AVX2 + FMA + F16C + BMI2 (Haswell, 2013+). Every Intel Mac that runs
#           macOS 13+ has these.
#   native: tune for the build machine. Local experiments only, never ship.
# ggml's runtime CPU-variant dispatch (GGML_CPU_ALL_VARIANTS) requires
# GGML_BACKEND_DL (dlopen'd per-variant shared libraries), which conflicts with
# the single static executable requirement, so one fixed level is compiled in
# and checked at startup by src/cpu_check.cpp.
# ---------------------------------------------------------------------------
set(SAYVANE_ARCH "${CMAKE_SYSTEM_PROCESSOR}")
if(APPLE AND CMAKE_OSX_ARCHITECTURES)
    list(LENGTH CMAKE_OSX_ARCHITECTURES _n_arch)
    if(_n_arch GREATER 1)
        message(FATAL_ERROR "Build one architecture at a time (got ${CMAKE_OSX_ARCHITECTURES}); combine with lipo afterwards.")
    endif()
    set(SAYVANE_ARCH "${CMAKE_OSX_ARCHITECTURES}")
endif()
string(TOLOWER "${SAYVANE_ARCH}" SAYVANE_ARCH)
if(SAYVANE_ARCH MATCHES "^(x86_64|amd64|x64)$")
    set(SAYVANE_IS_X86 ON)
    set(SAYVANE_ARCH_NAME "x86_64")
elseif(SAYVANE_ARCH MATCHES "^(arm64|aarch64)$")
    set(SAYVANE_IS_X86 OFF)
    set(SAYVANE_ARCH_NAME "aarch64")
else()
    message(FATAL_ERROR "Unsupported target architecture: ${SAYVANE_ARCH}")
endif()

if(APPLE)
    set(_default_level "avx2")
else()
    set(_default_level "sse42")
endif()
set(SAYVANE_X86_LEVEL "${_default_level}" CACHE STRING "x86-64 CPU level: sse42 | avx2 | native")
set_property(CACHE SAYVANE_X86_LEVEL PROPERTY STRINGS sse42 avx2 native)

# --- Engine build options (forced so the fetched project cannot override) ---
set(BUILD_SHARED_LIBS        OFF CACHE BOOL "" FORCE)
set(WHISPER_BUILD_TESTS      OFF CACHE BOOL "" FORCE)
set(WHISPER_BUILD_EXAMPLES   OFF CACHE BOOL "" FORCE)
set(WHISPER_BUILD_SERVER     OFF CACHE BOOL "" FORCE)
set(WHISPER_CURL             OFF CACHE BOOL "" FORCE)
set(WHISPER_SDL2             OFF CACHE BOOL "" FORCE)
set(WHISPER_COREML           OFF CACHE BOOL "" FORCE)
set(WHISPER_OPENVINO         OFF CACHE BOOL "" FORCE)
set(WHISPER_ALL_WARNINGS     OFF CACHE BOOL "" FORCE)
set(GGML_ALL_WARNINGS        OFF CACHE BOOL "" FORCE)
set(GGML_BACKEND_DL          OFF CACHE BOOL "" FORCE)
set(GGML_CPU_ALL_VARIANTS    OFF CACHE BOOL "" FORCE)
set(GGML_CCACHE              OFF CACHE BOOL "" FORCE)
set(GGML_CUDA                OFF CACHE BOOL "" FORCE)
set(GGML_HIP                 OFF CACHE BOOL "" FORCE)
set(GGML_VULKAN              OFF CACHE BOOL "" FORCE)
set(GGML_SYCL                OFF CACHE BOOL "" FORCE)
set(GGML_OPENCL              OFF CACHE BOOL "" FORCE)
set(GGML_WEBGPU              OFF CACHE BOOL "" FORCE)
# OpenMP would add a runtime DLL dependency (vcomp140.dll / libomp); ggml's own
# thread pool is used instead.
set(GGML_OPENMP              OFF CACHE BOOL "" FORCE)
set(GGML_LLAMAFILE           OFF CACHE BOOL "" FORCE)

if(SAYVANE_IS_X86 AND SAYVANE_X86_LEVEL STREQUAL "native")
    set(GGML_NATIVE ON CACHE BOOL "" FORCE)
else()
    set(GGML_NATIVE OFF CACHE BOOL "" FORCE)
endif()

set(SAYVANE_REQUIRED_CPU_FEATURES "")
if(SAYVANE_IS_X86)
    set(GGML_AVX512      OFF CACHE BOOL "" FORCE)
    set(GGML_AVX512_VBMI OFF CACHE BOOL "" FORCE)
    set(GGML_AVX512_VNNI OFF CACHE BOOL "" FORCE)
    set(GGML_AVX512_BF16 OFF CACHE BOOL "" FORCE)
    set(GGML_AVX_VNNI    OFF CACHE BOOL "" FORCE)
    set(GGML_AMX_TILE    OFF CACHE BOOL "" FORCE)
    set(GGML_AMX_INT8    OFF CACHE BOOL "" FORCE)
    set(GGML_AMX_BF16    OFF CACHE BOOL "" FORCE)
    if(SAYVANE_X86_LEVEL STREQUAL "avx2")
        foreach(_opt GGML_SSE42 GGML_AVX GGML_AVX2 GGML_FMA GGML_F16C GGML_BMI2)
            set(${_opt} ON CACHE BOOL "" FORCE)
        endforeach()
        set(SAYVANE_REQUIRED_CPU_FEATURES SAYVANE_REQUIRE_SSE42 SAYVANE_REQUIRE_AVX2)
    elseif(SAYVANE_X86_LEVEL STREQUAL "sse42")
        set(GGML_SSE42 ON CACHE BOOL "" FORCE)
        foreach(_opt GGML_AVX GGML_AVX2 GGML_FMA GGML_F16C GGML_BMI2)
            set(${_opt} OFF CACHE BOOL "" FORCE)
        endforeach()
        set(SAYVANE_REQUIRED_CPU_FEATURES SAYVANE_REQUIRE_SSE42)
    elseif(NOT SAYVANE_X86_LEVEL STREQUAL "native")
        message(FATAL_ERROR "SAYVANE_X86_LEVEL must be sse42, avx2 or native")
    endif()
endif()

if(APPLE)
    # Accelerate is a system framework: nothing extra ships with the app.
    set(GGML_ACCELERATE ON CACHE BOOL "" FORCE)
    set(GGML_BLAS       ON CACHE BOOL "" FORCE)
    set(GGML_BLAS_VENDOR "Apple" CACHE STRING "" FORCE)
    if(SAYVANE_ARCH_NAME STREQUAL "aarch64")
        # Apple Silicon: Metal with the shader library embedded in the binary.
        set(GGML_METAL               ON CACHE BOOL "" FORCE)
        set(GGML_METAL_EMBED_LIBRARY ON CACHE BOOL "" FORCE)
        # Every Apple Silicon Mac (M1 and later) supports ARMv8.4 + fp16 + dotprod.
        set(GGML_CPU_ARM_ARCH "armv8.4-a+fp16+dotprod" CACHE STRING "" FORCE)
    else()
        # Intel Macs: CPU only. Metal is never enabled just because the OS is macOS.
        set(GGML_METAL               OFF CACHE BOOL "" FORCE)
        set(GGML_METAL_EMBED_LIBRARY OFF CACHE BOOL "" FORCE)
    endif()
else()
    set(GGML_METAL OFF CACHE BOOL "" FORCE)
    set(GGML_BLAS  OFF CACHE BOOL "" FORCE)
endif()

FetchContent_Declare(
    sayvane_engine
    URL      "${SAYVANE_ENGINE_URL}"
    URL_HASH SHA256=${SAYVANE_ENGINE_SHA256}
    DOWNLOAD_EXTRACT_TIMESTAMP TRUE
)
FetchContent_MakeAvailable(sayvane_engine)

# ---------------------------------------------------------------------------
# Worker executable
# ---------------------------------------------------------------------------
add_executable(sayvane-speech
    src/main.cpp
    src/protocol.cpp
    src/engine.cpp
    src/wav_reader.cpp
    src/cpu_check.cpp
    src/platform.cpp
)

# nlohmann/json v3.12.0 single header, vendored (sha256 recorded in README.md).
target_include_directories(sayvane-speech PRIVATE src vendor)
target_link_libraries(sayvane-speech PRIVATE whisper)
target_compile_definitions(sayvane-speech PRIVATE
    SAYVANE_ENGINE_VERSION="${SAYVANE_ENGINE_VERSION}"
    SAYVANE_ARCH_NAME="${SAYVANE_ARCH_NAME}"
    SAYVANE_WORKER_VERSION="${PROJECT_VERSION}"
    ${SAYVANE_REQUIRED_CPU_FEATURES}
)
if(GGML_METAL)
    target_compile_definitions(sayvane-speech PRIVATE SAYVANE_HAS_METAL=1)
endif()

find_package(Threads REQUIRED)
target_link_libraries(sayvane-speech PRIVATE Threads::Threads)

if(MSVC)
    target_compile_options(sayvane-speech PRIVATE /W4 /utf-8 /EHsc)
    target_compile_definitions(sayvane-speech PRIVATE _CRT_SECURE_NO_WARNINGS NOMINMAX WIN32_LEAN_AND_MEAN)
else()
    target_compile_options(sayvane-speech PRIVATE -Wall -Wextra)
endif()

if(APPLE)
    # Strip local symbols in release; keeps no development paths in the binary's symbol table.
    set_target_properties(sayvane-speech PROPERTIES XCODE_ATTRIBUTE_DEAD_CODE_STRIPPING YES)
endif()

install(TARGETS sayvane-speech RUNTIME DESTINATION bin)
