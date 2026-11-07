#include "platform.h"

#include <chrono>
#include <thread>

#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#include <windows.h>
#else
#include <sys/stat.h>
#include <unistd.h>
#endif

namespace sayvane {

#ifdef _WIN32
static std::wstring utf8_to_wide(const std::string& s) {
    if (s.empty()) return std::wstring();
    int n = MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, s.data(), (int)s.size(), nullptr, 0);
    if (n <= 0) return std::wstring();
    std::wstring w((size_t)n, L'\0');
    MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, s.data(), (int)s.size(), &w[0], n);
    return w;
}
#endif

FILE* open_file_utf8(const std::string& path, const char* mode) {
#ifdef _WIN32
    std::wstring wpath = utf8_to_wide(path);
    if (wpath.empty()) return nullptr;
    std::wstring wmode;
    for (const char* m = mode; *m; ++m) wmode.push_back((wchar_t)*m);
    return _wfopen(wpath.c_str(), wmode.c_str());
#else
    return std::fopen(path.c_str(), mode);
#endif
}

bool regular_file_size(const std::string& path, unsigned long long* size_out) {
#ifdef _WIN32
    std::wstring wpath = utf8_to_wide(path);
    if (wpath.empty()) return false;
    WIN32_FILE_ATTRIBUTE_DATA data;
    if (!GetFileAttributesExW(wpath.c_str(), GetFileExInfoStandard, &data)) return false;
    if (data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) return false;
    if (size_out) {
        *size_out = ((unsigned long long)data.nFileSizeHigh << 32) | data.nFileSizeLow;
    }
    return true;
#else
    struct stat st;
    if (stat(path.c_str(), &st) != 0) return false;
    if (!S_ISREG(st.st_mode)) return false;
    if (size_out) *size_out = (unsigned long long)st.st_size;
    return true;
#endif
}

void set_stdio_binary() {
#ifdef _WIN32
    _setmode(_fileno(stdin), _O_BINARY);
    _setmode(_fileno(stdout), _O_BINARY);
#endif
}

void start_parent_watchdog(std::function<void()> on_orphaned) {
#ifdef _WIN32
    (void)on_orphaned;
#else
    const pid_t original_parent = getppid();
    std::thread([original_parent, on_orphaned]() {
        for (;;) {
            std::this_thread::sleep_for(std::chrono::milliseconds(1000));
            pid_t now = getppid();
            if (now != original_parent || now == 1) {
                on_orphaned();
                return;
            }
        }
    }).detach();
#endif
}

}  // namespace sayvane
