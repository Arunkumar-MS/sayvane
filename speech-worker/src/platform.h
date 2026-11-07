// Small platform helpers: UTF-8 file opening, stdio binary mode and
// parent-process lifecycle monitoring.
#pragma once

#include <cstdio>
#include <functional>
#include <string>

namespace sayvane {

// Opens a file whose path is UTF-8 encoded (uses _wfopen on Windows).
FILE* open_file_utf8(const std::string& path, const char* mode);

// Returns true if `path` names an existing regular file; fills size_out.
bool regular_file_size(const std::string& path, unsigned long long* size_out);

// Puts stdin/stdout into binary mode (no CRLF translation on Windows).
void set_stdio_binary();

// On macOS/Linux, starts a detached thread that invokes on_orphaned() when the
// parent process goes away (getppid() changes, e.g. to 1/launchd). No-op on
// Windows, where the parent owns the worker through a job object.
void start_parent_watchdog(std::function<void()> on_orphaned);

}  // namespace sayvane
