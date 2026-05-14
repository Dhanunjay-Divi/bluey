#define COBJMACROS
#define WIN32_LEAN_AND_MEAN
#define _WIN32_WINNT 0x0601

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

#define BLUEY_TARGET_SAMPLE_RATE 16000.0

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
} Args;

typedef struct Resampler {
    double ratio;
    double carry;
} Resampler;

static Args parse_args(int argc, char **argv) {
    Args args;
    args.source = CAPTURE_SOURCE_SYSTEM;
    args.duration_ms = 3000;
    args.continuous = 0;

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--source") == 0 && i + 1 < argc) {
            i++;
            if (strcmp(argv[i], "microphone") == 0) {
                args.source = CAPTURE_SOURCE_MICROPHONE;
            } else {
                args.source = CAPTURE_SOURCE_SYSTEM;
            }
        } else if (strcmp(argv[i], "--duration-ms") == 0 && i + 1 < argc) {
            i++;
            long value = strtol(argv[i], NULL, 10);
            if (value < 250) {
                value = 250;
            }
            if (value > 30000) {
                value = 30000;
            }
            args.duration_ms = (DWORD)value;
        } else if (strcmp(argv[i], "--continuous") == 0) {
            args.continuous = 1;
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

static void write_resampled_i16(Resampler *resampler, float sample) {
    resampler->carry += resampler->ratio;
    while (resampler->carry >= 1.0) {
        int16_t out = (int16_t)(sample * 32767.0f);
        fwrite(&out, sizeof(int16_t), 1, stdout);
        resampler->carry -= 1.0;
    }
}

static void write_frames_as_16k_mono_i16(
    Resampler *resampler,
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
        write_resampled_i16(resampler, mono);
    }
}

static int fail_hr(const char *label, HRESULT hr) {
    fprintf(stderr, "bluey windows audio helper failed: %s (0x%08lx)\n", label, (unsigned long)hr);
    return 1;
}

int main(int argc, char **argv) {
    Args args = parse_args(argc, argv);
    _setmode(_fileno(stdout), _O_BINARY);

    HRESULT hr = CoInitializeEx(NULL, COINIT_MULTITHREADED);
    if (FAILED(hr)) {
        return fail_hr("CoInitializeEx", hr);
    }

    IMMDeviceEnumerator *enumerator = NULL;
    IMMDevice *device = NULL;
    IAudioClient *audio_client = NULL;
    IAudioCaptureClient *capture_client = NULL;
    WAVEFORMATEX *mix_format = NULL;
    int exit_code = 1;

    hr = CoCreateInstance(
        &CLSID_MMDeviceEnumerator,
        NULL,
        CLSCTX_ALL,
        &IID_IMMDeviceEnumerator,
        (void **)&enumerator
    );
    if (FAILED(hr)) {
        exit_code = fail_hr("CoCreateInstance IMMDeviceEnumerator", hr);
        goto cleanup;
    }

    EDataFlow flow = args.source == CAPTURE_SOURCE_SYSTEM ? eRender : eCapture;
    hr = IMMDeviceEnumerator_GetDefaultAudioEndpoint(enumerator, flow, eConsole, &device);
    if (FAILED(hr)) {
        exit_code = fail_hr("GetDefaultAudioEndpoint", hr);
        goto cleanup;
    }

    hr = IMMDevice_Activate(device, &IID_IAudioClient, CLSCTX_ALL, NULL, (void **)&audio_client);
    if (FAILED(hr)) {
        exit_code = fail_hr("Activate IAudioClient", hr);
        goto cleanup;
    }

    hr = IAudioClient_GetMixFormat(audio_client, &mix_format);
    if (FAILED(hr)) {
        exit_code = fail_hr("GetMixFormat", hr);
        goto cleanup;
    }

    DWORD stream_flags = args.source == CAPTURE_SOURCE_SYSTEM ? AUDCLNT_STREAMFLAGS_LOOPBACK : 0;
    REFERENCE_TIME buffer_duration = 10000000;
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
        exit_code = fail_hr("IAudioClient Initialize", hr);
        goto cleanup;
    }

    hr = IAudioClient_GetService(audio_client, &IID_IAudioCaptureClient, (void **)&capture_client);
    if (FAILED(hr)) {
        exit_code = fail_hr("GetService IAudioCaptureClient", hr);
        goto cleanup;
    }

    Resampler resampler;
    resampler.ratio = BLUEY_TARGET_SAMPLE_RATE / (double)mix_format->nSamplesPerSec;
    resampler.carry = 0.0;

    hr = IAudioClient_Start(audio_client);
    if (FAILED(hr)) {
        exit_code = fail_hr("IAudioClient Start", hr);
        goto cleanup;
    }

    ULONGLONG end_tick = args.continuous ? ULLONG_MAX : (GetTickCount64() + args.duration_ms);
    while (GetTickCount64() < end_tick) {
        UINT32 packet_frames = 0;
        hr = IAudioCaptureClient_GetNextPacketSize(capture_client, &packet_frames);
        if (FAILED(hr)) {
            exit_code = fail_hr("GetNextPacketSize", hr);
            goto stop;
        }

        if (packet_frames == 0) {
            Sleep(5);
            continue;
        }

        while (packet_frames > 0) {
            BYTE *data = NULL;
            UINT32 frame_count = 0;
            DWORD flags = 0;
            hr = IAudioCaptureClient_GetBuffer(capture_client, &data, &frame_count, &flags, NULL, NULL);
            if (FAILED(hr)) {
                exit_code = fail_hr("GetBuffer", hr);
                goto stop;
            }

            write_frames_as_16k_mono_i16(&resampler, data, frame_count, flags, mix_format);
            fflush(stdout);
            IAudioCaptureClient_ReleaseBuffer(capture_client, frame_count);

            hr = IAudioCaptureClient_GetNextPacketSize(capture_client, &packet_frames);
            if (FAILED(hr)) {
                exit_code = fail_hr("GetNextPacketSize after buffer", hr);
                goto stop;
            }
        }
    }

    fflush(stdout);
    exit_code = 0;

stop:
    IAudioClient_Stop(audio_client);

cleanup:
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
