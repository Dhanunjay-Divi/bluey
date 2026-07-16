#define COBJMACROS
#define WIN32_LEAN_AND_MEAN
#ifndef _WIN32_WINNT
#define _WIN32_WINNT 0x0A00
#endif

#include <initguid.h>
#include <audioclient.h>
#include <fcntl.h>
#include <ksmedia.h>
#include <limits.h>
#include <mmdeviceapi.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <windows.h>
#include <io.h>

#include "audio_args.h"
#include "resampler.h"

static const GUID BLUEY_SUBTYPE_PCM = {
    0x00000001,
    0x0000,
    0x0010,
    {0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71}
};
static const GUID BLUEY_SUBTYPE_IEEE_FLOAT = {
    0x00000003,
    0x0000,
    0x0010,
    {0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71}
};

#ifdef _MSC_VER
DEFINE_GUID(CLSID_MMDeviceEnumerator, 0xbcde0395, 0xe52f, 0x467c, 0x8e, 0x3d, 0xc4, 0x57, 0x92, 0x91, 0x69, 0x2e);
DEFINE_GUID(IID_IMMDeviceEnumerator, 0xa95664d2, 0x9614, 0x4f35, 0xa7, 0x46, 0xde, 0x8d, 0xb6, 0x36, 0x17, 0xe6);
DEFINE_GUID(IID_IAudioClient, 0x1cb9ad4c, 0xdbfa, 0x4c32, 0xb1, 0x78, 0xc2, 0xf5, 0x68, 0xa7, 0x03, 0xb2);
DEFINE_GUID(IID_IAudioCaptureClient, 0xc8adbd64, 0xe71e, 0x48a0, 0xa4, 0xde, 0x18, 0x5c, 0x39, 0x5c, 0xd3, 0x17);
#endif

static const char *source_name(BlueyCaptureSource source) {
    return source == BLUEY_CAPTURE_SOURCE_MICROPHONE ? "microphone" : "system";
}

static void emit_ready(BlueyCaptureSource source) {
    fprintf(
        stderr,
        "{\"event\":\"ready\",\"protocol_version\":1,\"source\":\"%s\",\"backend\":\"wasapi\",\"format\":{\"sample_rate_hz\":16000,\"channel_count\":1,\"sample_format\":\"i16_le\"}}\n",
        source_name(source)
    );
    fflush(stderr);
}

static void emit_stopped(BlueyCaptureSource source, const char *reason, int exit_code) {
    fprintf(
        stderr,
        "{\"event\":\"stopped\",\"protocol_version\":1,\"source\":\"%s\",\"reason\":\"%s\",\"exit_code\":%d}\n",
        source_name(source),
        reason,
        exit_code
    );
    fflush(stderr);
}

static void emit_error(
    BlueyCaptureSource source,
    const char *code,
    const char *operation,
    int recoverable
) {
    fprintf(
        stderr,
        "{\"event\":\"error\",\"protocol_version\":1,\"source\":\"%s\",\"code\":\"%s\",\"operation\":\"%s\",\"recoverable\":%s}\n",
        source_name(source),
        code,
        operation,
        recoverable ? "true" : "false"
    );
    fflush(stderr);
}

static int fail_hr(BlueyCaptureSource source, const char *operation, HRESULT hr) {
    if (hr == E_ACCESSDENIED) {
        fprintf(
            stderr,
            "{\"event\":\"error\",\"protocol_version\":1,\"source\":\"%s\",\"code\":\"permission_denied\",\"permission\":\"%s\",\"operation\":\"%s\",\"native_code\":\"0x%08lX\",\"recoverable\":false}\n",
            source_name(source),
            source == BLUEY_CAPTURE_SOURCE_MICROPHONE ? "microphone" : "system_audio",
            operation,
            (unsigned long)hr
        );
        fflush(stderr);
        return 3;
    }

    int recoverable = hr == AUDCLNT_E_DEVICE_INVALIDATED
        || hr == AUDCLNT_E_SERVICE_NOT_RUNNING
        || hr == AUDCLNT_E_RESOURCES_INVALIDATED;
    fprintf(
        stderr,
        "{\"event\":\"error\",\"protocol_version\":1,\"source\":\"%s\",\"code\":\"wasapi_hresult\",\"operation\":\"%s\",\"native_code\":\"0x%08lX\",\"recoverable\":%s}\n",
        source_name(source),
        operation,
        (unsigned long)hr,
        recoverable ? "true" : "false"
    );
    fflush(stderr);
    return 1;
}

static void emit_win32_error(
    BlueyCaptureSource source,
    const char *code,
    const char *operation,
    DWORD native_code,
    int recoverable
) {
    fprintf(
        stderr,
        "{\"event\":\"error\",\"protocol_version\":1,\"source\":\"%s\",\"code\":\"%s\",\"operation\":\"%s\",\"native_code\":%lu,\"recoverable\":%s}\n",
        source_name(source),
        code,
        operation,
        (unsigned long)native_code,
        recoverable ? "true" : "false"
    );
    fflush(stderr);
}

static int guid_equals(const GUID *left, const GUID *right) {
    return memcmp(left, right, sizeof(GUID)) == 0;
}

static const GUID *extensible_subtype(const WAVEFORMATEX *format) {
    if (format->wFormatTag != WAVE_FORMAT_EXTENSIBLE
        || format->cbSize < (WORD)(sizeof(WAVEFORMATEXTENSIBLE) - sizeof(WAVEFORMATEX))) {
        return NULL;
    }
    const WAVEFORMATEXTENSIBLE *extensible = (const WAVEFORMATEXTENSIBLE *)format;
    return &extensible->SubFormat;
}

static int format_is_float(const WAVEFORMATEX *format) {
    if (format->wFormatTag == WAVE_FORMAT_IEEE_FLOAT) {
        return 1;
    }
    const GUID *subtype = extensible_subtype(format);
    return subtype != NULL && guid_equals(subtype, &BLUEY_SUBTYPE_IEEE_FLOAT);
}

static int format_is_pcm(const WAVEFORMATEX *format) {
    if (format->wFormatTag == WAVE_FORMAT_PCM) {
        return 1;
    }
    const GUID *subtype = extensible_subtype(format);
    return subtype != NULL && guid_equals(subtype, &BLUEY_SUBTYPE_PCM);
}

static int format_is_supported(const WAVEFORMATEX *format) {
    if (format == NULL
        || format->nSamplesPerSec < 8000U
        || format->nSamplesPerSec > 384000U
        || format->nChannels == 0
        || format->nBlockAlign == 0) {
        return 0;
    }

    int is_float = format_is_float(format);
    int is_pcm = format_is_pcm(format);
    if ((!is_float && !is_pcm)
        || (is_float && format->wBitsPerSample != 32)
        || (is_pcm
            && format->wBitsPerSample != 8
            && format->wBitsPerSample != 16
            && format->wBitsPerSample != 24
            && format->wBitsPerSample != 32)) {
        return 0;
    }

    if (format->wFormatTag == WAVE_FORMAT_EXTENSIBLE) {
        const WAVEFORMATEXTENSIBLE *extensible = (const WAVEFORMATEXTENSIBLE *)format;
        WORD valid_bits = extensible->Samples.wValidBitsPerSample;
        if (valid_bits != 0 && valid_bits > format->wBitsPerSample) {
            return 0;
        }
    }

    uint32_t bytes_per_sample = ((uint32_t)format->wBitsPerSample + 7U) / 8U;
    uint32_t expected_block_align = (uint32_t)format->nChannels * bytes_per_sample;
    uint64_t expected_average = (uint64_t)format->nSamplesPerSec * expected_block_align;
    return expected_block_align == (uint32_t)format->nBlockAlign
        && expected_average <= UINT32_MAX
        && (uint32_t)expected_average == format->nAvgBytesPerSec;
}

static float clamp_float(float value) {
    if (value != value) {
        return 0.0f;
    }
    if (value > 1.0f) {
        return 1.0f;
    }
    if (value < -1.0f) {
        return -1.0f;
    }
    return value;
}

static float read_channel_sample(const BYTE *sample, const WAVEFORMATEX *format, WORD channel) {
    WORD bytes_per_sample = (WORD)(((uint32_t)format->wBitsPerSample + 7U) / 8U);
    const BYTE *data = sample + ((size_t)channel * bytes_per_sample);

    if (format_is_float(format)) {
        float value = 0.0f;
        memcpy(&value, data, sizeof(value));
        return clamp_float(value);
    }

    switch (format->wBitsPerSample) {
    case 8:
        return clamp_float(((float)data[0] - 128.0f) / 128.0f);
    case 16: {
        int16_t value = 0;
        memcpy(&value, data, sizeof(value));
        return clamp_float((float)value / 32768.0f);
    }
    case 24: {
        int32_t value = (int32_t)data[0]
            | ((int32_t)data[1] << 8)
            | ((int32_t)data[2] << 16);
        if ((value & 0x00800000) != 0) {
            value |= (int32_t)0xff000000;
        }
        return clamp_float((float)value / 8388608.0f);
    }
    case 32: {
        int32_t value = 0;
        memcpy(&value, data, sizeof(value));
        return clamp_float((float)value / 2147483648.0f);
    }
    default:
        return 0.0f;
    }
}

static int write_stdout_sample(int16_t sample, void *context) {
    (void)context;
    return fwrite(&sample, sizeof(sample), 1, stdout) == 1;
}

static int write_frames_as_16k_mono_i16(
    BlueyResampler *resampler,
    const BYTE *data,
    UINT32 frame_count,
    DWORD flags,
    const WAVEFORMATEX *format
) {
    WORD channel_count = format->nChannels;
    WORD block_align = format->nBlockAlign;

    for (UINT32 frame = 0; frame < frame_count; frame++) {
        float mono = 0.0f;
        if ((flags & AUDCLNT_BUFFERFLAGS_SILENT) == 0 && data != NULL) {
            const BYTE *frame_data = data + ((size_t)frame * block_align);
            for (WORD channel = 0; channel < channel_count; channel++) {
                mono += read_channel_sample(frame_data, format, channel) / (float)channel_count;
            }
        }
        if (!bluey_resampler_push(resampler, mono, write_stdout_sample, NULL)) {
            return 0;
        }
    }
    return 1;
}

int main(int argc, char **argv) {
    BlueyAudioArgs args;
    BlueyAudioArgsStatus args_status =
        bluey_audio_parse_args(argc, (const char *const *)argv, &args);
    if (args_status != BLUEY_AUDIO_ARGS_OK) {
        fprintf(
            stderr,
            "{\"event\":\"error\",\"protocol_version\":1,\"code\":\"invalid_arguments\",\"detail\":\"%s\",\"usage\":\"--source system|microphone [--duration-ms 250..30000|--continuous]\",\"recoverable\":false}\n",
            bluey_audio_args_status_code(args_status)
        );
        fflush(stderr);
        return 2;
    }

    if (_setmode(_fileno(stdout), _O_BINARY) == -1) {
        emit_error(args.source, "stdout_binary_mode_failed", "_setmode", 0);
        return 1;
    }

    HRESULT hr = CoInitializeEx(NULL, COINIT_MULTITHREADED);
    if (FAILED(hr)) {
        return fail_hr(args.source, "CoInitializeEx", hr);
    }

    IMMDeviceEnumerator *enumerator = NULL;
    IMMDevice *device = NULL;
    IAudioClient *audio_client = NULL;
    IAudioCaptureClient *capture_client = NULL;
    WAVEFORMATEX *mix_format = NULL;
    HANDLE capture_event = NULL;
    int started = 0;
    int exit_code = 1;
    const char *stop_reason = "capture_error";

    hr = CoCreateInstance(
        &CLSID_MMDeviceEnumerator,
        NULL,
        CLSCTX_ALL,
        &IID_IMMDeviceEnumerator,
        (void **)&enumerator
    );
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "CoCreateInstance", hr);
        goto cleanup;
    }

    EDataFlow flow =
        args.source == BLUEY_CAPTURE_SOURCE_SYSTEM ? eRender : eCapture;
    hr = IMMDeviceEnumerator_GetDefaultAudioEndpoint(enumerator, flow, eConsole, &device);
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "GetDefaultAudioEndpoint", hr);
        goto cleanup;
    }

    hr = IMMDevice_Activate(
        device,
        &IID_IAudioClient,
        CLSCTX_ALL,
        NULL,
        (void **)&audio_client
    );
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "ActivateAudioClient", hr);
        goto cleanup;
    }

    hr = IAudioClient_GetMixFormat(audio_client, &mix_format);
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "GetMixFormat", hr);
        goto cleanup;
    }
    if (!format_is_supported(mix_format)) {
        emit_error(args.source, "unsupported_mix_format", "GetMixFormat", 0);
        exit_code = 1;
        goto cleanup;
    }

    DWORD stream_flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
    if (args.source == BLUEY_CAPTURE_SOURCE_SYSTEM) {
        stream_flags |= AUDCLNT_STREAMFLAGS_LOOPBACK;
    }
    /* A 100 ms shared buffer absorbs scheduler jitter; event wakeups keep reads prompt. */
    REFERENCE_TIME buffer_duration = 1000000;
    hr = IAudioClient_Initialize(
        audio_client,
        AUDCLNT_SHAREMODE_SHARED,
        stream_flags,
        buffer_duration,
        0,
        mix_format,
        NULL
    );
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "InitializeAudioClient", hr);
        goto cleanup;
    }

    capture_event = CreateEventW(NULL, FALSE, FALSE, NULL);
    if (capture_event == NULL) {
        emit_win32_error(
            args.source,
            "event_create_failed",
            "CreateEventW",
            GetLastError(),
            0
        );
        exit_code = 1;
        goto cleanup;
    }
    hr = IAudioClient_SetEventHandle(audio_client, capture_event);
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "SetEventHandle", hr);
        goto cleanup;
    }

    hr = IAudioClient_GetService(
        audio_client,
        &IID_IAudioCaptureClient,
        (void **)&capture_client
    );
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "GetCaptureClient", hr);
        goto cleanup;
    }

    BlueyResampler resampler;
    if (!bluey_resampler_init(&resampler, (double)mix_format->nSamplesPerSec)) {
        emit_error(args.source, "resampler_init_failed", "bluey_resampler_init", 0);
        exit_code = 1;
        goto cleanup;
    }

    hr = IAudioClient_Start(audio_client);
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "StartAudioClient", hr);
        goto cleanup;
    }
    started = 1;
    emit_ready(args.source);

    ULONGLONG end_tick =
        args.continuous ? ULLONG_MAX : GetTickCount64() + (ULONGLONG)args.duration_ms;
    while (GetTickCount64() < end_tick) {
        DWORD wait_ms = 1000;
        if (!args.continuous) {
            ULONGLONG now = GetTickCount64();
            if (now >= end_tick) {
                break;
            }
            ULONGLONG remaining = end_tick - now;
            if (remaining < (ULONGLONG)wait_ms) {
                wait_ms = (DWORD)remaining;
            }
        }

        DWORD wait_result = WaitForSingleObject(capture_event, wait_ms);
        if (wait_result == WAIT_TIMEOUT) {
            continue;
        }
        if (wait_result != WAIT_OBJECT_0) {
            emit_win32_error(
                args.source,
                "event_wait_failed",
                "WaitForSingleObject",
                wait_result == WAIT_FAILED ? GetLastError() : wait_result,
                1
            );
            exit_code = 1;
            stop_reason = "capture_error";
            goto stop;
        }

        UINT32 packet_frames = 0;
        hr = IAudioCaptureClient_GetNextPacketSize(capture_client, &packet_frames);
        if (FAILED(hr)) {
            exit_code = fail_hr(args.source, "GetNextPacketSize", hr);
            stop_reason = "capture_error";
            goto stop;
        }

        while (packet_frames > 0) {
            BYTE *data = NULL;
            UINT32 frame_count = 0;
            DWORD flags = 0;
            hr = IAudioCaptureClient_GetBuffer(
                capture_client,
                &data,
                &frame_count,
                &flags,
                NULL,
                NULL
            );
            if (FAILED(hr)) {
                exit_code = fail_hr(args.source, "GetBuffer", hr);
                stop_reason = "capture_error";
                goto stop;
            }

            int output_open =
                write_frames_as_16k_mono_i16(&resampler, data, frame_count, flags, mix_format)
                && fflush(stdout) == 0
                && !ferror(stdout);
            hr = IAudioCaptureClient_ReleaseBuffer(capture_client, frame_count);
            if (FAILED(hr)) {
                exit_code = fail_hr(args.source, "ReleaseBuffer", hr);
                stop_reason = "capture_error";
                goto stop;
            }
            if (!output_open) {
                exit_code = 0;
                stop_reason = "stdout_closed";
                goto stop;
            }

            hr = IAudioCaptureClient_GetNextPacketSize(capture_client, &packet_frames);
            if (FAILED(hr)) {
                exit_code = fail_hr(args.source, "GetNextPacketSize", hr);
                stop_reason = "capture_error";
                goto stop;
            }
        }
    }

    if (fflush(stdout) != 0 || ferror(stdout)) {
        exit_code = 0;
        stop_reason = "stdout_closed";
    } else {
        exit_code = 0;
        stop_reason = "duration_complete";
    }

stop:
    if (started) {
        hr = IAudioClient_Stop(audio_client);
        if (FAILED(hr)) {
            exit_code = fail_hr(args.source, "StopAudioClient", hr);
            stop_reason = "stop_error";
        }
        emit_stopped(args.source, stop_reason, exit_code);
    }

cleanup:
    if (capture_event != NULL) {
        CloseHandle(capture_event);
    }
    if (mix_format != NULL) {
        CoTaskMemFree(mix_format);
    }
    if (capture_client != NULL) {
        IAudioCaptureClient_Release(capture_client);
    }
    if (audio_client != NULL) {
        IAudioClient_Release(audio_client);
    }
    if (device != NULL) {
        IMMDevice_Release(device);
    }
    if (enumerator != NULL) {
        IMMDeviceEnumerator_Release(enumerator);
    }
    CoUninitialize();
    return exit_code;
}
