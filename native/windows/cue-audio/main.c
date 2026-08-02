#define COBJMACROS
#define WIN32_LEAN_AND_MEAN
#define _WIN32_WINNT 0x0A00

#include <initguid.h>
#include <audioclient.h>
#include <fcntl.h>
#include <ksmedia.h>
#include <math.h>
#include <mmdeviceapi.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <windows.h>
#include <io.h>

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

typedef enum CaptureSource {
    CAPTURE_SOURCE_SYSTEM,
    CAPTURE_SOURCE_MICROPHONE
} CaptureSource;

typedef struct Args {
    CaptureSource source;
    DWORD duration_ms;
    int continuous;
    int valid;
} Args;

static Args parse_args(int argc, char **argv) {
    Args args;
    args.source = CAPTURE_SOURCE_SYSTEM;
    args.duration_ms = 3000;
    args.continuous = 0;
    args.valid = 1;

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--source") == 0 && i + 1 < argc) {
            i++;
            if (strcmp(argv[i], "microphone") == 0) {
                args.source = CAPTURE_SOURCE_MICROPHONE;
            } else if (strcmp(argv[i], "system") == 0) {
                args.source = CAPTURE_SOURCE_SYSTEM;
            } else {
                args.valid = 0;
            }
        } else if (strcmp(argv[i], "--duration-ms") == 0 && i + 1 < argc) {
            i++;
            char *end = NULL;
            long value = strtol(argv[i], &end, 10);
            if (end == argv[i] || *end != '\0') {
                args.valid = 0;
                continue;
            }
            if (value < 250) {
                value = 250;
            }
            if (value > 30000) {
                value = 30000;
            }
            args.duration_ms = (DWORD)value;
        } else if (strcmp(argv[i], "--continuous") == 0) {
            args.continuous = 1;
        } else {
            args.valid = 0;
        }
    }

    return args;
}

static int guid_equals(const GUID *left, const GUID *right) {
    return memcmp(left, right, sizeof(GUID)) == 0;
}

static int format_is_float(const WAVEFORMATEX *format) {
    if (format->wFormatTag == WAVE_FORMAT_IEEE_FLOAT) {
        return 1;
    }
    if (format->wFormatTag == WAVE_FORMAT_EXTENSIBLE) {
        const WAVEFORMATEXTENSIBLE *extensible = (const WAVEFORMATEXTENSIBLE *)format;
        return guid_equals(&extensible->SubFormat, &BLUEY_SUBTYPE_IEEE_FLOAT);
    }
    return 0;
}

static int format_is_pcm(const WAVEFORMATEX *format) {
    if (format->wFormatTag == WAVE_FORMAT_PCM) {
        return 1;
    }
    if (format->wFormatTag == WAVE_FORMAT_EXTENSIBLE) {
        const WAVEFORMATEXTENSIBLE *extensible = (const WAVEFORMATEXTENSIBLE *)format;
        return guid_equals(&extensible->SubFormat, &BLUEY_SUBTYPE_PCM);
    }
    return 0;
}

static float clamp_float(float value) {
    if (value > 1.0f) {
        return 1.0f;
    }
    if (value < -1.0f) {
        return -1.0f;
    }
    return value;
}

static float read_channel_sample(const BYTE *sample, const WAVEFORMATEX *format, WORD channel) {
    WORD bytes_per_sample = (WORD)((format->wBitsPerSample + 7) / 8);
    const BYTE *data = sample + (channel * bytes_per_sample);

    if (format_is_float(format) && format->wBitsPerSample == 32) {
        float value = 0.0f;
        memcpy(&value, data, sizeof(float));
        return clamp_float(value);
    }

    if (!format_is_pcm(format)) {
        return 0.0f;
    }

    switch (format->wBitsPerSample) {
    case 8:
        return clamp_float(((float)data[0] - 128.0f) / 128.0f);
    case 16: {
        int16_t value = 0;
        memcpy(&value, data, sizeof(int16_t));
        return clamp_float((float)value / 32768.0f);
    }
    case 24: {
        int32_t value = ((int32_t)data[0]) | ((int32_t)data[1] << 8) | ((int32_t)data[2] << 16);
        if (value & 0x00800000) {
            value |= (int32_t)0xff000000;
        }
        return clamp_float((float)value / 8388608.0f);
    }
    case 32: {
        int32_t value = 0;
        memcpy(&value, data, sizeof(int32_t));
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
    const WORD channel_count = format->nChannels == 0 ? 1 : format->nChannels;
    const WORD block_align = format->nBlockAlign;

    for (UINT32 frame = 0; frame < frame_count; frame++) {
        float mono = 0.0f;
        if ((flags & AUDCLNT_BUFFERFLAGS_SILENT) == 0 && data != NULL) {
            const BYTE *frame_data = data + (frame * block_align);
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

static const char *source_name(CaptureSource source) {
    return source == CAPTURE_SOURCE_MICROPHONE ? "microphone" : "system";
}

static void emit_ready(CaptureSource source) {
    fprintf(
        stderr,
        "{\"event\":\"ready\",\"source\":\"%s\",\"backend\":\"wasapi\",\"format\":{\"sample_rate_hz\":16000,\"channel_count\":1,\"sample_format\":\"i16\"}}\n",
        source_name(source)
    );
    fflush(stderr);
}

static void emit_stopped(CaptureSource source, const char *reason) {
    fprintf(
        stderr,
        "{\"event\":\"stopped\",\"source\":\"%s\",\"reason\":\"%s\"}\n",
        source_name(source),
        reason
    );
    fflush(stderr);
}

static int fail_hr(CaptureSource source, const char *label, HRESULT hr) {
    if (hr == E_ACCESSDENIED) {
        fprintf(
            stderr,
            "{\"event\":\"permission_denied\",\"source\":\"%s\",\"permission\":\"%s\",\"message\":\"audio capture access denied\"}\n",
            source_name(source),
            source == CAPTURE_SOURCE_MICROPHONE ? "microphone" : "system_audio"
        );
        fflush(stderr);
        return 3;
    }
    const int recoverable = hr == AUDCLNT_E_DEVICE_INVALIDATED
        || hr == AUDCLNT_E_SERVICE_NOT_RUNNING
        || hr == AUDCLNT_E_RESOURCES_INVALIDATED;
    fprintf(
        stderr,
        "{\"event\":\"error\",\"source\":\"%s\",\"code\":\"hresult_%08lx\",\"message\":\"%s\",\"recoverable\":%s}\n",
        source_name(source),
        (unsigned long)hr,
        label,
        recoverable ? "true" : "false"
    );
    fflush(stderr);
    return 1;
}

int main(int argc, char **argv) {
    Args args = parse_args(argc, argv);
    if (!args.valid) {
        fprintf(
            stderr,
            "{\"event\":\"error\",\"code\":\"invalid_arguments\",\"message\":\"expected --source system|microphone, --duration-ms 250..30000, or --continuous\",\"recoverable\":false}\n"
        );
        fflush(stderr);
        return 2;
    }
    _setmode(_fileno(stdout), _O_BINARY);

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
    int exit_code = 1;

    hr = CoCreateInstance(
        &CLSID_MMDeviceEnumerator,
        NULL,
        CLSCTX_ALL,
        &IID_IMMDeviceEnumerator,
        (void **)&enumerator
    );
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "CoCreateInstance IMMDeviceEnumerator", hr);
        goto cleanup;
    }

    EDataFlow flow = args.source == CAPTURE_SOURCE_SYSTEM ? eRender : eCapture;
    hr = IMMDeviceEnumerator_GetDefaultAudioEndpoint(enumerator, flow, eConsole, &device);
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "GetDefaultAudioEndpoint", hr);
        goto cleanup;
    }

    hr = IMMDevice_Activate(device, &IID_IAudioClient, CLSCTX_ALL, NULL, (void **)&audio_client);
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "Activate IAudioClient", hr);
        goto cleanup;
    }

    hr = IAudioClient_GetMixFormat(audio_client, &mix_format);
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "GetMixFormat", hr);
        goto cleanup;
    }
    if (mix_format->nSamplesPerSec < 8000 || mix_format->nSamplesPerSec > 384000
        || mix_format->nChannels == 0
        || mix_format->nBlockAlign == 0
        || (!format_is_float(mix_format) && !format_is_pcm(mix_format))) {
        fprintf(
            stderr,
            "{\"event\":\"error\",\"source\":\"%s\",\"code\":\"unsupported_mix_format\",\"message\":\"WASAPI returned an unsupported mix format\",\"recoverable\":false}\n",
            source_name(args.source)
        );
        fflush(stderr);
        exit_code = 1;
        goto cleanup;
    }

    DWORD stream_flags = AUDCLNT_STREAMFLAGS_EVENTCALLBACK;
    if (args.source == CAPTURE_SOURCE_SYSTEM) {
        stream_flags |= AUDCLNT_STREAMFLAGS_LOOPBACK;
    }
    /* 100 ms protects against scheduler jitter while event wakeups preserve low latency. */
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
        exit_code = fail_hr(args.source, "IAudioClient Initialize", hr);
        goto cleanup;
    }

    capture_event = CreateEventW(NULL, FALSE, FALSE, NULL);
    if (capture_event == NULL) {
        fprintf(
            stderr,
            "{\"event\":\"error\",\"source\":\"%s\",\"code\":\"event_create_failed\",\"message\":\"could not create WASAPI event\",\"recoverable\":false}\n",
            source_name(args.source)
        );
        fflush(stderr);
        exit_code = 1;
        goto cleanup;
    }
    hr = IAudioClient_SetEventHandle(audio_client, capture_event);
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "SetEventHandle", hr);
        goto cleanup;
    }

    hr = IAudioClient_GetService(audio_client, &IID_IAudioCaptureClient, (void **)&capture_client);
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "GetService IAudioCaptureClient", hr);
        goto cleanup;
    }

    BlueyResampler resampler;
    if (!bluey_resampler_init(&resampler, (double)mix_format->nSamplesPerSec)) {
        exit_code = 1;
        goto cleanup;
    }

    hr = IAudioClient_Start(audio_client);
    if (FAILED(hr)) {
        exit_code = fail_hr(args.source, "IAudioClient Start", hr);
        goto cleanup;
    }
    emit_ready(args.source);

    ULONGLONG end_tick = args.continuous ? ULLONG_MAX : (GetTickCount64() + args.duration_ms);
    while (GetTickCount64() < end_tick) {
        DWORD wait_ms = 1000;
        if (!args.continuous) {
            ULONGLONG now = GetTickCount64();
            if (now >= end_tick) {
                break;
            }
            ULONGLONG remaining = end_tick - now;
            wait_ms = (DWORD)(remaining < wait_ms ? remaining : wait_ms);
        }
        DWORD wait_result = WaitForSingleObject(capture_event, wait_ms);
        if (wait_result == WAIT_TIMEOUT) {
            continue;
        }
        if (wait_result != WAIT_OBJECT_0) {
            fprintf(
                stderr,
                "{\"event\":\"error\",\"source\":\"%s\",\"code\":\"event_wait_failed\",\"message\":\"WASAPI event wait failed\",\"recoverable\":true}\n",
                source_name(args.source)
            );
            fflush(stderr);
            exit_code = 1;
            goto stop;
        }

        UINT32 packet_frames = 0;
        hr = IAudioCaptureClient_GetNextPacketSize(capture_client, &packet_frames);
        if (FAILED(hr)) {
            exit_code = fail_hr(args.source, "GetNextPacketSize", hr);
            goto stop;
        }

        while (packet_frames > 0) {
            BYTE *data = NULL;
            UINT32 frame_count = 0;
            DWORD flags = 0;
            hr = IAudioCaptureClient_GetBuffer(capture_client, &data, &frame_count, &flags, NULL, NULL);
            if (FAILED(hr)) {
                exit_code = fail_hr(args.source, "GetBuffer", hr);
                goto stop;
            }

            if (!write_frames_as_16k_mono_i16(
                    &resampler, data, frame_count, flags, mix_format
                ) || fflush(stdout) != 0 || ferror(stdout)) {
                IAudioCaptureClient_ReleaseBuffer(capture_client, frame_count);
                fprintf(
                    stderr,
                    "{\"event\":\"error\",\"source\":\"%s\",\"code\":\"stdout_closed\",\"message\":\"audio consumer closed the stream\",\"recoverable\":false}\n",
                    source_name(args.source)
                );
                fflush(stderr);
                exit_code = 0;
                goto stop;
            }
            hr = IAudioCaptureClient_ReleaseBuffer(capture_client, frame_count);
            if (FAILED(hr)) {
                exit_code = fail_hr(args.source, "ReleaseBuffer", hr);
                goto stop;
            }

            hr = IAudioCaptureClient_GetNextPacketSize(capture_client, &packet_frames);
            if (FAILED(hr)) {
                exit_code = fail_hr(args.source, "GetNextPacketSize after buffer", hr);
                goto stop;
            }
        }
    }

    fflush(stdout);
    exit_code = 0;
    emit_stopped(args.source, "duration_complete");

stop:
    IAudioClient_Stop(audio_client);

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
