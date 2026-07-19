#ifndef BLUEY_WINDOWS_CAPTURE_CONTRACT_H_
#define BLUEY_WINDOWS_CAPTURE_CONTRACT_H_

#include <cerrno>
#include <cstdint>
#include <cwchar>
#include <limits>

namespace bluey_capture {

constexpr std::uint64_t kBytesPerPixel = 4;
constexpr std::uint64_t kMaxCapturePixels = 100'000'000;
constexpr std::uint64_t kMaxCaptureBytes =
    kMaxCapturePixels * kBytesPerPixel;
constexpr std::uint64_t kMaxOutputBytes = 512ULL * 1024ULL * 1024ULL;

struct CaptureRect {
  int x;
  int y;
  int width;
  int height;
};

inline wchar_t ascii_lower(wchar_t value) {
  return value >= L'A' && value <= L'Z' ? value + (L'a' - L'A') : value;
}

inline bool is_absolute_png_output_path(const wchar_t* path) {
  if (path == nullptr) {
    return false;
  }
  const std::size_t length = std::wcslen(path);
  if (length < 7 || length > 32'000 ||
      (path[0] == L'\\' && path[1] == L'\\') ||
      (path[0] == L'/' && path[1] == L'/')) {
    return false;
  }
  const bool drive_absolute =
      ((path[0] >= L'A' && path[0] <= L'Z') ||
       (path[0] >= L'a' && path[0] <= L'z')) &&
      path[1] == L':' && (path[2] == L'\\' || path[2] == L'/');
  if (!drive_absolute) {
    return false;
  }
  const wchar_t* extension = std::wcsrchr(path, L'.');
  return extension != nullptr && std::wcslen(extension) == 4 &&
         ascii_lower(extension[0]) == L'.' &&
         ascii_lower(extension[1]) == L'p' &&
         ascii_lower(extension[2]) == L'n' &&
         ascii_lower(extension[3]) == L'g';
}

inline bool parse_int32(const wchar_t* value, int* result) {
  if (value == nullptr || result == nullptr || value[0] == L'\0') {
    return false;
  }
  errno = 0;
  wchar_t* end = nullptr;
  const long long parsed = std::wcstoll(value, &end, 10);
  if (errno == ERANGE || end == value || end == nullptr || *end != L'\0' ||
      parsed < (std::numeric_limits<int>::min)() ||
      parsed > (std::numeric_limits<int>::max)()) {
    return false;
  }
  *result = static_cast<int>(parsed);
  return true;
}

inline bool checked_capture_size(int width, int height,
                                 std::uint64_t* pixel_count,
                                 std::uint64_t* byte_count) {
  if (width <= 0 || height <= 0 || pixel_count == nullptr ||
      byte_count == nullptr) {
    return false;
  }
  const std::uint64_t pixels = static_cast<std::uint64_t>(width) *
                               static_cast<std::uint64_t>(height);
  if (pixels > kMaxCapturePixels ||
      pixels > kMaxCaptureBytes / kBytesPerPixel) {
    return false;
  }
  *pixel_count = pixels;
  *byte_count = pixels * kBytesPerPixel;
  return true;
}

inline bool rect_is_within_virtual_desktop(const CaptureRect& region,
                                           const CaptureRect& desktop) {
  std::uint64_t region_pixels = 0;
  std::uint64_t region_bytes = 0;
  std::uint64_t desktop_pixels = 0;
  std::uint64_t desktop_bytes = 0;
  if (!checked_capture_size(region.width, region.height, &region_pixels,
                            &region_bytes) ||
      !checked_capture_size(desktop.width, desktop.height, &desktop_pixels,
                            &desktop_bytes)) {
    return false;
  }

  const std::int64_t region_left = region.x;
  const std::int64_t region_top = region.y;
  const std::int64_t region_right =
      region_left + static_cast<std::int64_t>(region.width);
  const std::int64_t region_bottom =
      region_top + static_cast<std::int64_t>(region.height);
  const std::int64_t desktop_left = desktop.x;
  const std::int64_t desktop_top = desktop.y;
  const std::int64_t desktop_right =
      desktop_left + static_cast<std::int64_t>(desktop.width);
  const std::int64_t desktop_bottom =
      desktop_top + static_cast<std::int64_t>(desktop.height);

  return region_left >= desktop_left && region_top >= desktop_top &&
         region_right <= desktop_right && region_bottom <= desktop_bottom;
}

}  // namespace bluey_capture

#endif  // BLUEY_WINDOWS_CAPTURE_CONTRACT_H_
