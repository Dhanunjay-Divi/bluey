#include "../capture_contract.h"

#include <cstdio>
#include <limits>

namespace {

int failures = 0;

void expect(bool condition, const char* message) {
  if (!condition) {
    std::fprintf(stderr, "capture contract test failed: %s\n", message);
    ++failures;
  }
}

}  // namespace

int main() {
  expect(bluey_capture::is_absolute_png_output_path(
             L"C:\\Bluey\\capture.png"),
         "absolute PNG output path accepted");
  expect(bluey_capture::is_absolute_png_output_path(
             L"d:/Bluey/capture.PNG"),
         "absolute PNG extension is case insensitive");
  expect(!bluey_capture::is_absolute_png_output_path(L"C"),
         "short output path rejected safely");
  expect(!bluey_capture::is_absolute_png_output_path(L"capture.png"),
         "relative output path rejected");
  expect(!bluey_capture::is_absolute_png_output_path(
             L"\\\\server\\share\\capture.png"),
         "UNC output path rejected");
  expect(!bluey_capture::is_absolute_png_output_path(
             L"C:\\Bluey\\capture.png:stream"),
         "alternate data stream output rejected");

  int parsed = 0;
  expect(bluey_capture::parse_int32(L"-1920", &parsed) && parsed == -1920,
         "negative monitor coordinate parses");
  expect(!bluey_capture::parse_int32(L"", &parsed), "empty integer rejected");
  expect(!bluey_capture::parse_int32(L"12px", &parsed),
         "integer suffix rejected");
  expect(!bluey_capture::parse_int32(L"999999999999999999999", &parsed),
         "overflowing integer rejected");

  std::uint64_t pixels = 0;
  std::uint64_t bytes = 0;
  expect(bluey_capture::checked_capture_size(10'000, 10'000, &pixels, &bytes),
         "maximum bounded capture accepted");
  expect(pixels == bluey_capture::kMaxCapturePixels,
         "maximum pixel count is exact");
  expect(bytes == bluey_capture::kMaxCaptureBytes,
         "maximum byte count is exact");
  expect(!bluey_capture::checked_capture_size(10'001, 10'000, &pixels, &bytes),
         "oversized capture rejected");
  expect(!bluey_capture::checked_capture_size(0, 100, &pixels, &bytes),
         "zero width rejected");
  expect(!bluey_capture::checked_capture_size(-1, 100, &pixels, &bytes),
         "negative width rejected");

  const bluey_capture::CaptureRect desktop{-1920, -1080, 5760, 3240};
  expect(bluey_capture::rect_is_within_virtual_desktop(
             bluey_capture::CaptureRect{-1920, -1080, 1920, 1080}, desktop),
         "negative-coordinate monitor region accepted");
  expect(bluey_capture::rect_is_within_virtual_desktop(
             bluey_capture::CaptureRect{0, 0, 3840, 2160}, desktop),
         "desktop right and bottom boundary accepted");
  expect(!bluey_capture::rect_is_within_virtual_desktop(
             bluey_capture::CaptureRect{-1921, -1080, 1920, 1080}, desktop),
         "region left of virtual desktop rejected");
  expect(!bluey_capture::rect_is_within_virtual_desktop(
             bluey_capture::CaptureRect{0, 0, 3841, 2160}, desktop),
         "region beyond virtual desktop rejected");
  expect(!bluey_capture::rect_is_within_virtual_desktop(
             bluey_capture::CaptureRect{
                 std::numeric_limits<int>::max() - 10, 0, 20, 20},
             desktop),
         "large coordinate outside desktop rejected without overflow");

  if (failures != 0) {
    return 1;
  }
  std::puts("capture contract tests passed");
  return 0;
}
