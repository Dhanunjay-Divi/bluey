#define WIN32_LEAN_AND_MEAN
#define _WIN32_WINNT 0x0A00

#include <windows.h>
#include <propidl.h>
#include <gdiplus.h>

#include <cstdint>
#include <cstdio>
#include <cwchar>
#include <limits>

namespace {

constexpr std::int64_t kMaxCapturePixels = 250'000'000;

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
  BitmapHandle(HDC compatible, int width, int height)
      : handle_(CreateCompatibleBitmap(compatible, width, height)) {}
  ~BitmapHandle() {
    if (handle_ != nullptr) {
      DeleteObject(handle_);
    }
  }
  HBITMAP get() const { return handle_; }

 private:
  HBITMAP handle_ = nullptr;
};

void emit_error(const char* code) {
  std::fprintf(stderr,
               "{\"event\":\"error\",\"code\":\"%s\","
               "\"recoverable\":false}\n",
               code);
  std::fflush(stderr);
}

bool png_encoder_clsid(CLSID* result) {
  UINT count = 0;
  UINT bytes = 0;
  if (Gdiplus::GetImageEncodersSize(&count, &bytes) != Gdiplus::Ok || count == 0 ||
      bytes == 0 || bytes > 1024 * 1024) {
    return false;
  }

  auto* encoders = static_cast<Gdiplus::ImageCodecInfo*>(HeapAlloc(GetProcessHeap(), 0, bytes));
  if (encoders == nullptr) {
    return false;
  }
  const auto status = Gdiplus::GetImageEncoders(count, bytes, encoders);
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
  set_context.raw = GetProcAddress(user32, "SetProcessDpiAwarenessContext");
  if (set_context.typed != nullptr) {
    // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, represented as a negative
    // pseudo-handle by the Windows SDK.
    set_context.typed(reinterpret_cast<HANDLE>(static_cast<INT_PTR>(-4)));
  }
}

bool output_path_is_acceptable(const wchar_t* path) {
  if (path == nullptr || path[0] == L'\0' || std::wcslen(path) > 32'000) {
    return false;
  }
  if ((path[0] == L'\\' && path[1] == L'\\') ||
      (path[0] == L'/' && path[1] == L'/')) {
    return false;
  }
  const wchar_t* extension = std::wcsrchr(path, L'.');
  return extension != nullptr && _wcsicmp(extension, L".png") == 0;
}

int capture_virtual_desktop(const wchar_t* output_path) {
  enable_per_monitor_dpi_awareness();

  const int left = GetSystemMetrics(SM_XVIRTUALSCREEN);
  const int top = GetSystemMetrics(SM_YVIRTUALSCREEN);
  const int width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
  const int height = GetSystemMetrics(SM_CYVIRTUALSCREEN);
  if (width <= 0 || height <= 0 ||
      static_cast<std::int64_t>(width) * static_cast<std::int64_t>(height) >
          kMaxCapturePixels) {
    emit_error("invalid_virtual_desktop_bounds");
    return 3;
  }

  GdiplusSession gdiplus;
  ScreenDc screen;
  if (!gdiplus.ready() || screen.get() == nullptr) {
    emit_error("capture_runtime_unavailable");
    return 4;
  }
  MemoryDc memory(screen.get());
  BitmapHandle bitmap(screen.get(), width, height);
  if (memory.get() == nullptr || bitmap.get() == nullptr) {
    emit_error("capture_buffer_unavailable");
    return 5;
  }

  HGDIOBJ previous = SelectObject(memory.get(), bitmap.get());
  if (previous == nullptr || previous == HGDI_ERROR) {
    emit_error("capture_buffer_select_failed");
    return 6;
  }
  const BOOL copied = BitBlt(memory.get(), 0, 0, width, height, screen.get(), left, top,
                             SRCCOPY | CAPTUREBLT);
  SelectObject(memory.get(), previous);
  if (!copied) {
    emit_error("desktop_copy_failed");
    return 7;
  }

  CLSID encoder{};
  if (!png_encoder_clsid(&encoder)) {
    emit_error("png_encoder_unavailable");
    return 8;
  }
  Gdiplus::Bitmap image(bitmap.get(), nullptr);
  if (image.GetLastStatus() != Gdiplus::Ok ||
      image.Save(output_path, &encoder, nullptr) != Gdiplus::Ok) {
    emit_error("png_write_failed");
    return 9;
  }

  std::fprintf(stderr,
               "{\"event\":\"captured\",\"format\":\"png\","
               "\"width\":%d,\"height\":%d}\n",
               width, height);
  std::fflush(stderr);
  return 0;
}

}  // namespace

int wmain(int argc, wchar_t** argv) {
  if (argc != 3 || std::wcscmp(argv[1], L"--screenshot") != 0 ||
      !output_path_is_acceptable(argv[2])) {
    emit_error("invalid_arguments");
    return 2;
  }
  return capture_virtual_desktop(argv[2]);
}
