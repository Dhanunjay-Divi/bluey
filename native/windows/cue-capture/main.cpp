#define WIN32_LEAN_AND_MEAN
#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0601
#endif

#include <windows.h>
#include <propidl.h>
#include <gdiplus.h>

#include <cstdint>
#include <cstdio>
#include <cwchar>

#include "capture_contract.h"

namespace {

class GdiplusSession {
 public:
  GdiplusSession() {
    Gdiplus::GdiplusStartupInput input;
    ready_ = Gdiplus::GdiplusStartup(&token_, &input, nullptr) == Gdiplus::Ok;
  }

  ~GdiplusSession() {
    if (ready_) {
      Gdiplus::GdiplusShutdown(token_);
    }
  }

  bool ready() const { return ready_; }

 private:
  ULONG_PTR token_ = 0;
  bool ready_ = false;
};

class ScreenDc {
 public:
  ScreenDc() : handle_(GetDC(nullptr)) {}

  ~ScreenDc() {
    if (handle_ != nullptr) {
      ReleaseDC(nullptr, handle_);
    }
  }

  HDC get() const { return handle_; }

 private:
  HDC handle_ = nullptr;
};

class MemoryDc {
 public:
  explicit MemoryDc(HDC compatible) : handle_(CreateCompatibleDC(compatible)) {}

  ~MemoryDc() {
    if (handle_ != nullptr) {
      DeleteDC(handle_);
    }
  }

  HDC get() const { return handle_; }

 private:
  HDC handle_ = nullptr;
};

class BitmapHandle {
 public:
  BitmapHandle(HDC compatible, int width, int height) {
    BITMAPINFO info{};
    info.bmiHeader.biSize = sizeof(BITMAPINFOHEADER);
    info.bmiHeader.biWidth = width;
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;
    void* pixels = nullptr;
    handle_ = CreateDIBSection(compatible, &info, DIB_RGB_COLORS, &pixels,
                               nullptr, 0);
  }

  ~BitmapHandle() {
    if (handle_ != nullptr) {
      DeleteObject(handle_);
    }
  }

  HBITMAP get() const { return handle_; }

 private:
  HBITMAP handle_ = nullptr;
};

class SelectedBitmap {
 public:
  SelectedBitmap(HDC dc, HBITMAP bitmap) : dc_(dc) {
    previous_ = SelectObject(dc_, bitmap);
    ready_ = previous_ != nullptr && previous_ != HGDI_ERROR;
  }

  ~SelectedBitmap() {
    if (ready_) {
      SelectObject(dc_, previous_);
    }
  }

  bool ready() const { return ready_; }

 private:
  HDC dc_ = nullptr;
  HGDIOBJ previous_ = nullptr;
  bool ready_ = false;
};

void emit_error(const char* code, bool recoverable, DWORD win32_error) {
  std::fprintf(stderr,
               "{\"event\":\"error\",\"code\":\"%s\","
               "\"recoverable\":%s,\"win32_error\":%lu}\n",
               code, recoverable ? "true" : "false",
               static_cast<unsigned long>(win32_error));
  std::fflush(stderr);
}

bool png_encoder_clsid(CLSID* result) {
  UINT count = 0;
  UINT bytes = 0;
  if (result == nullptr ||
      Gdiplus::GetImageEncodersSize(&count, &bytes) != Gdiplus::Ok ||
      count == 0 || bytes == 0 || bytes > 1024U * 1024U) {
    return false;
  }

  auto* encoders = static_cast<Gdiplus::ImageCodecInfo*>(
      HeapAlloc(GetProcessHeap(), 0, bytes));
  if (encoders == nullptr) {
    return false;
  }
  const Gdiplus::Status status =
      Gdiplus::GetImageEncoders(count, bytes, encoders);
  bool found = false;
  if (status == Gdiplus::Ok) {
    for (UINT index = 0; index < count; ++index) {
      if (encoders[index].MimeType != nullptr &&
          std::wcscmp(encoders[index].MimeType, L"image/png") == 0) {
        *result = encoders[index].Clsid;
        found = true;
        break;
      }
    }
  }
  HeapFree(GetProcessHeap(), 0, encoders);
  return found;
}

void enable_per_monitor_dpi_awareness() {
  using SetDpiContext = BOOL(WINAPI*)(HANDLE);
  HMODULE user32 = GetModuleHandleW(L"user32.dll");
  if (user32 == nullptr) {
    return;
  }
  union {
    FARPROC raw;
    SetDpiContext typed;
  } set_context{};
  set_context.raw =
      GetProcAddress(user32, "SetProcessDpiAwarenessContext");
  if (set_context.typed != nullptr &&
      set_context.typed(
          reinterpret_cast<HANDLE>(static_cast<INT_PTR>(-4))) != FALSE) {
    return;
  }
  SetProcessDPIAware();
}

bool output_file_size(const wchar_t* path, std::uint64_t* result) {
  if (result == nullptr) {
    return false;
  }
  WIN32_FILE_ATTRIBUTE_DATA attributes{};
  if (GetFileAttributesExW(path, GetFileExInfoStandard, &attributes) == FALSE) {
    return false;
  }
  ULARGE_INTEGER size{};
  size.HighPart = attributes.nFileSizeHigh;
  size.LowPart = attributes.nFileSizeLow;
  *result = size.QuadPart;
  return true;
}

int capture_desktop(const wchar_t* output_path,
                    const bluey_capture::CaptureRect* requested_region) {
  const ULONGLONG started_at = GetTickCount64();
  enable_per_monitor_dpi_awareness();

  const bluey_capture::CaptureRect desktop{
      GetSystemMetrics(SM_XVIRTUALSCREEN),
      GetSystemMetrics(SM_YVIRTUALSCREEN),
      GetSystemMetrics(SM_CXVIRTUALSCREEN),
      GetSystemMetrics(SM_CYVIRTUALSCREEN)};
  std::uint64_t desktop_pixels = 0;
  std::uint64_t desktop_bytes = 0;
  if (!bluey_capture::checked_capture_size(
          desktop.width, desktop.height, &desktop_pixels, &desktop_bytes)) {
    emit_error("invalid_virtual_desktop_bounds", false, GetLastError());
    return 3;
  }

  const bluey_capture::CaptureRect capture =
      requested_region == nullptr ? desktop : *requested_region;
  if (!bluey_capture::rect_is_within_virtual_desktop(capture, desktop)) {
    emit_error("invalid_capture_region", false, ERROR_INVALID_PARAMETER);
    return 4;
  }
  std::uint64_t pixel_count = 0;
  std::uint64_t pixel_bytes = 0;
  if (!bluey_capture::checked_capture_size(
          capture.width, capture.height, &pixel_count, &pixel_bytes)) {
    emit_error("capture_bounds_exceed_limit", false,
               ERROR_NOT_ENOUGH_MEMORY);
    return 5;
  }

  const DWORD existing_attributes = GetFileAttributesW(output_path);
  if (existing_attributes != INVALID_FILE_ATTRIBUTES) {
    emit_error("output_path_exists", false, ERROR_FILE_EXISTS);
    return 6;
  }

  GdiplusSession gdiplus;
  ScreenDc screen;
  if (!gdiplus.ready() || screen.get() == nullptr) {
    emit_error("capture_runtime_unavailable", true, GetLastError());
    return 7;
  }
  MemoryDc memory(screen.get());
  BitmapHandle bitmap(screen.get(), capture.width, capture.height);
  if (memory.get() == nullptr || bitmap.get() == nullptr) {
    emit_error("capture_buffer_unavailable", true, GetLastError());
    return 8;
  }

  {
    SelectedBitmap selected(memory.get(), bitmap.get());
    if (!selected.ready()) {
      emit_error("capture_buffer_select_failed", true, GetLastError());
      return 9;
    }
    if (BitBlt(memory.get(), 0, 0, capture.width, capture.height,
               screen.get(), capture.x, capture.y,
               SRCCOPY | CAPTUREBLT) == FALSE) {
      emit_error("desktop_copy_failed", true, GetLastError());
      return 10;
    }
  }

  CLSID encoder{};
  if (!png_encoder_clsid(&encoder)) {
    emit_error("png_encoder_unavailable", true, GetLastError());
    return 11;
  }
  Gdiplus::Bitmap image(bitmap.get(), nullptr);
  if (image.GetLastStatus() != Gdiplus::Ok ||
      image.Save(output_path, &encoder, nullptr) != Gdiplus::Ok) {
    DeleteFileW(output_path);
    emit_error("png_write_failed", true, GetLastError());
    return 12;
  }

  std::uint64_t output_bytes = 0;
  if (!output_file_size(output_path, &output_bytes) || output_bytes == 0 ||
      output_bytes > bluey_capture::kMaxOutputBytes) {
    DeleteFileW(output_path);
    emit_error("png_size_invalid", false, ERROR_FILE_TOO_LARGE);
    return 13;
  }

  const ULONGLONG elapsed_ms = GetTickCount64() - started_at;
  std::fprintf(
      stderr,
      "{\"event\":\"captured\",\"format\":\"png\","
      "\"x\":%d,\"y\":%d,\"width\":%d,\"height\":%d,"
      "\"pixel_bytes\":%llu,\"output_bytes\":%llu,"
      "\"elapsed_ms\":%llu}\n",
      capture.x, capture.y, capture.width, capture.height,
      static_cast<unsigned long long>(pixel_bytes),
      static_cast<unsigned long long>(output_bytes),
      static_cast<unsigned long long>(elapsed_ms));
  std::fflush(stderr);
  return 0;
}

}  // namespace

int wmain(int argc, wchar_t** argv) {
  if ((argc != 3 && argc != 8) ||
      std::wcscmp(argv[1], L"--screenshot") != 0 ||
      !bluey_capture::is_absolute_png_output_path(argv[2])) {
    emit_error("invalid_arguments", false, ERROR_INVALID_PARAMETER);
    return 2;
  }

  bluey_capture::CaptureRect region{};
  const bluey_capture::CaptureRect* requested_region = nullptr;
  if (argc == 8) {
    if (std::wcscmp(argv[3], L"--region") != 0 ||
        !bluey_capture::parse_int32(argv[4], &region.x) ||
        !bluey_capture::parse_int32(argv[5], &region.y) ||
        !bluey_capture::parse_int32(argv[6], &region.width) ||
        !bluey_capture::parse_int32(argv[7], &region.height)) {
      emit_error("invalid_region_arguments", false, ERROR_INVALID_PARAMETER);
      return 2;
    }
    requested_region = &region;
  }
  return capture_desktop(argv[2], requested_region);
}
