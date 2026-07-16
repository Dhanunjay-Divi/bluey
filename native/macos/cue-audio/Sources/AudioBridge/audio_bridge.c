#include "audio_bridge.h"

#include "resampler.h"

#include <limits.h>
#include <math.h>
#include <stdalign.h>
#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>

#if ATOMIC_INT_LOCK_FREE != 2 || ATOMIC_LLONG_LOCK_FREE != 2
#error "cue-audio requires lock-free C11 atomics on its supported macOS architectures"
#endif

#define BLUEY_AUDIO_RING_CAPACITY 16U
#define BLUEY_AUDIO_MAX_PACKET_FRAMES 4096U
#define BLUEY_AUDIO_MAX_CHANNELS 8U
#define BLUEY_AUDIO_MAX_BUFFERS 8U
#define BLUEY_AUDIO_MAX_BYTES_PER_SAMPLE 4U
#define BLUEY_AUDIO_MAX_PACKET_BYTES \
    (BLUEY_AUDIO_MAX_PACKET_FRAMES * BLUEY_AUDIO_MAX_CHANNELS \
     * BLUEY_AUDIO_MAX_BYTES_PER_SAMPLE)
#define BLUEY_AUDIO_MAX_OUTPUT_SAMPLES 16384U

typedef enum BlueyPacketSampleFormat {
    BLUEY_PACKET_FORMAT_F32 = 1,
    BLUEY_PACKET_FORMAT_S16 = 2
} BlueyPacketSampleFormat;

typedef struct BlueyAudioPacket {
    double sample_rate;
    uint32_t frame_count;
    uint32_t channel_count;
    uint32_t buffer_count;
    uint32_t sample_size;
    BlueyPacketSampleFormat sample_format;
    uint32_t buffer_offsets[BLUEY_AUDIO_MAX_BUFFERS];
    uint32_t buffer_channels[BLUEY_AUDIO_MAX_BUFFERS];
    alignas(16) uint8_t payload[BLUEY_AUDIO_MAX_PACKET_BYTES];
} BlueyAudioPacket;

struct BlueyAudioBridge {
    atomic_ullong write_sequence;
    atomic_ullong read_sequence;
    atomic_ullong dropped_packets;
    atomic_ullong invalid_packets;
    atomic_int stop_requested;
    atomic_int capture_failure;
    atomic_int worker_failure;
    atomic_int output_closed;
    alignas(64) BlueyAudioPacket packets[BLUEY_AUDIO_RING_CAPACITY];
};

struct BlueyAudioProcessor {
    BlueyResampler resampler;
    double source_rate;
    int initialized;
};

typedef struct BlueyAudioBufferListStorage {
    UInt32 mNumberBuffers;
    AudioBuffer mBuffers[BLUEY_AUDIO_MAX_BUFFERS];
} BlueyAudioBufferListStorage;

typedef struct BlueyOutputSink {
    int16_t *samples;
    size_t capacity;
    size_t count;
} BlueyOutputSink;

static void increment_counter(atomic_ullong *counter) {
    (void)atomic_fetch_add_explicit(counter, 1ULL, memory_order_relaxed);
}

BlueyAudioBridge *bluey_audio_bridge_create(void) {
    BlueyAudioBridge *bridge = calloc(1U, sizeof(*bridge));
    if (bridge == NULL) {
        return NULL;
    }
    atomic_init(&bridge->write_sequence, 0ULL);
    atomic_init(&bridge->read_sequence, 0ULL);
    atomic_init(&bridge->dropped_packets, 0ULL);
    atomic_init(&bridge->invalid_packets, 0ULL);
    atomic_init(&bridge->stop_requested, 0);
    atomic_init(&bridge->capture_failure, 0);
    atomic_init(&bridge->worker_failure, BLUEY_AUDIO_WORKER_FAILURE_NONE);
    atomic_init(&bridge->output_closed, 0);
    return bridge;
}

void bluey_audio_bridge_destroy(BlueyAudioBridge *bridge) {
    free(bridge);
}

BlueyAudioProcessor *bluey_audio_processor_create(void) {
    return calloc(1U, sizeof(BlueyAudioProcessor));
}

void bluey_audio_processor_destroy(BlueyAudioProcessor *processor) {
    free(processor);
}

static int classify_format(
    const AudioStreamBasicDescription *format,
    BlueyPacketSampleFormat *sample_format,
    uint32_t *sample_size
) {
    if (format == NULL || sample_format == NULL || sample_size == NULL
        || format->mFormatID != kAudioFormatLinearPCM
        || !isfinite(format->mSampleRate)
        || format->mSampleRate < 8000.0
        || format->mSampleRate > 384000.0
        || format->mChannelsPerFrame == 0U
        || format->mChannelsPerFrame > BLUEY_AUDIO_MAX_CHANNELS
        || format->mFramesPerPacket != 1U
        || (format->mFormatFlags & kAudioFormatFlagIsPacked) == 0U
        || (format->mFormatFlags & kAudioFormatFlagIsBigEndian) != 0U) {
        return 0;
    }

    if (
        (format->mFormatFlags & kAudioFormatFlagIsFloat) != 0U
        && format->mBitsPerChannel == 32U
    ) {
        *sample_format = BLUEY_PACKET_FORMAT_F32;
        *sample_size = 4U;
        return 1;
    }
    if (
        (format->mFormatFlags & kAudioFormatFlagIsSignedInteger) != 0U
        && format->mBitsPerChannel == 16U
    ) {
        *sample_format = BLUEY_PACKET_FORMAT_S16;
        *sample_size = 2U;
        return 1;
    }
    return 0;
}

static int validate_buffer_list(
    const AudioBufferList *buffer_list,
    uint32_t frame_count,
    const AudioStreamBasicDescription *format,
    BlueyPacketSampleFormat *sample_format,
    uint32_t *sample_size
) {
    if (buffer_list == NULL || frame_count == 0U
        || buffer_list->mNumberBuffers == 0U
        || buffer_list->mNumberBuffers > BLUEY_AUDIO_MAX_BUFFERS
        || !classify_format(format, sample_format, sample_size)) {
        return 0;
    }

    uint32_t total_channels = 0U;
    for (uint32_t index = 0U; index < buffer_list->mNumberBuffers; index++) {
        const AudioBuffer *buffer = &buffer_list->mBuffers[index];
        if (buffer->mData == NULL || buffer->mNumberChannels == 0U
            || buffer->mNumberChannels > BLUEY_AUDIO_MAX_CHANNELS
            || total_channels > BLUEY_AUDIO_MAX_CHANNELS - buffer->mNumberChannels) {
            return 0;
        }
        total_channels += buffer->mNumberChannels;
    }
    return total_channels == format->mChannelsPerFrame;
}

static int publish_packet(
    BlueyAudioBridge *bridge,
    const AudioBufferList *buffer_list,
    uint32_t source_frame_offset,
    uint32_t frame_count,
    const AudioStreamBasicDescription *format,
    BlueyPacketSampleFormat sample_format,
    uint32_t sample_size
) {
    unsigned long long write_sequence = atomic_load_explicit(
        &bridge->write_sequence,
        memory_order_relaxed
    );
    unsigned long long read_sequence = atomic_load_explicit(
        &bridge->read_sequence,
        memory_order_acquire
    );
    if (write_sequence - read_sequence >= BLUEY_AUDIO_RING_CAPACITY) {
        increment_counter(&bridge->dropped_packets);
        return 0;
    }

    size_t packet_index =
        (size_t)(write_sequence % (unsigned long long)BLUEY_AUDIO_RING_CAPACITY);
    BlueyAudioPacket *packet = &bridge->packets[packet_index];
    packet->sample_rate = format->mSampleRate;
    packet->frame_count = frame_count;
    packet->channel_count = format->mChannelsPerFrame;
    packet->buffer_count = buffer_list->mNumberBuffers;
    packet->sample_size = sample_size;
    packet->sample_format = sample_format;

    size_t payload_offset = 0U;
    for (uint32_t index = 0U; index < buffer_list->mNumberBuffers; index++) {
        const AudioBuffer *buffer = &buffer_list->mBuffers[index];
        size_t channels = (size_t)buffer->mNumberChannels;
        size_t frame_stride = channels * (size_t)sample_size;
        size_t source_offset = (size_t)source_frame_offset * frame_stride;
        size_t copy_size = (size_t)frame_count * frame_stride;
        size_t source_size = (size_t)buffer->mDataByteSize;

        if (source_offset > source_size || copy_size > source_size - source_offset
            || payload_offset > BLUEY_AUDIO_MAX_PACKET_BYTES
            || copy_size > BLUEY_AUDIO_MAX_PACKET_BYTES - payload_offset
            || payload_offset > UINT32_MAX) {
            increment_counter(&bridge->invalid_packets);
            return 0;
        }

        packet->buffer_offsets[index] = (uint32_t)payload_offset;
        packet->buffer_channels[index] = buffer->mNumberChannels;
        const uint8_t *source = (const uint8_t *)buffer->mData + source_offset;
        memcpy(packet->payload + payload_offset, source, copy_size);
        payload_offset += copy_size;
    }

    atomic_store_explicit(
        &bridge->write_sequence,
        write_sequence + 1ULL,
        memory_order_release
    );
    return 1;
}

uint32_t bluey_audio_bridge_push_audio_buffer_list(
    BlueyAudioBridge *bridge,
    const AudioBufferList *buffer_list,
    uint32_t frame_count,
    const AudioStreamBasicDescription *format
) {
    if (bridge == NULL || bluey_audio_bridge_stop_requested(bridge)) {
        return 0U;
    }

    BlueyPacketSampleFormat sample_format = BLUEY_PACKET_FORMAT_F32;
    uint32_t sample_size = 0U;
    if (!validate_buffer_list(
            buffer_list,
            frame_count,
            format,
            &sample_format,
            &sample_size
        )) {
        increment_counter(&bridge->invalid_packets);
        return 0U;
    }

    uint32_t enqueued = 0U;
    uint32_t offset = 0U;
    while (offset < frame_count) {
        uint32_t remaining = frame_count - offset;
        uint32_t chunk_frames = remaining < BLUEY_AUDIO_MAX_PACKET_FRAMES
            ? remaining
            : BLUEY_AUDIO_MAX_PACKET_FRAMES;
        if (publish_packet(
                bridge,
                buffer_list,
                offset,
                chunk_frames,
                format,
                sample_format,
                sample_size
            )) {
            enqueued += 1U;
        }
        offset += chunk_frames;
    }
    return enqueued;
}

uint32_t bluey_audio_bridge_push_sample_buffer(
    BlueyAudioBridge *bridge,
    CMSampleBufferRef sample_buffer
) {
    if (bridge == NULL || sample_buffer == NULL
        || !CMSampleBufferIsValid(sample_buffer)
        || bluey_audio_bridge_stop_requested(bridge)) {
        return 0U;
    }

    CMItemCount sample_count = CMSampleBufferGetNumSamples(sample_buffer);
    if (sample_count <= 0 || (uint64_t)sample_count > UINT32_MAX) {
        increment_counter(&bridge->invalid_packets);
        return 0U;
    }
    CMFormatDescriptionRef format_description =
        CMSampleBufferGetFormatDescription(sample_buffer);
    const AudioStreamBasicDescription *format =
        format_description == NULL
        ? NULL
        : CMAudioFormatDescriptionGetStreamBasicDescription(format_description);
    if (format == NULL) {
        increment_counter(&bridge->invalid_packets);
        return 0U;
    }

    BlueyAudioBufferListStorage storage;
    memset(&storage, 0, sizeof(storage));
    CMBlockBufferRef retained_block_buffer = NULL;
    OSStatus status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
        sample_buffer,
        NULL,
        (AudioBufferList *)&storage,
        sizeof(storage),
        NULL,
        NULL,
        kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment,
        &retained_block_buffer
    );
    if (status != noErr) {
        if (retained_block_buffer != NULL) {
            CFRelease(retained_block_buffer);
        }
        increment_counter(&bridge->invalid_packets);
        return 0U;
    }

    uint32_t enqueued = bluey_audio_bridge_push_audio_buffer_list(
        bridge,
        (const AudioBufferList *)&storage,
        (uint32_t)sample_count,
        format
    );
    if (retained_block_buffer != NULL) {
        CFRelease(retained_block_buffer);
    }
    return enqueued;
}

uint32_t bluey_audio_bridge_push_interleaved_f32(
    BlueyAudioBridge *bridge,
    const float *samples,
    uint32_t frame_count,
    uint32_t channel_count,
    double sample_rate
) {
    if (bridge == NULL || samples == NULL || frame_count == 0U
        || channel_count == 0U || channel_count > BLUEY_AUDIO_MAX_CHANNELS) {
        if (bridge != NULL) {
            increment_counter(&bridge->invalid_packets);
        }
        return 0U;
    }

    uint64_t byte_count = (uint64_t)frame_count
        * (uint64_t)channel_count
        * (uint64_t)sizeof(float);
    if (byte_count > UINT32_MAX) {
        increment_counter(&bridge->invalid_packets);
        return 0U;
    }

    AudioBufferList buffer_list;
    memset(&buffer_list, 0, sizeof(buffer_list));
    buffer_list.mNumberBuffers = 1U;
    buffer_list.mBuffers[0].mNumberChannels = channel_count;
    buffer_list.mBuffers[0].mDataByteSize = (UInt32)byte_count;
    buffer_list.mBuffers[0].mData = (void *)samples;

    AudioStreamBasicDescription format;
    memset(&format, 0, sizeof(format));
    format.mSampleRate = sample_rate;
    format.mFormatID = kAudioFormatLinearPCM;
    format.mFormatFlags = kAudioFormatFlagsNativeFloatPacked;
    format.mBytesPerPacket = channel_count * (UInt32)sizeof(float);
    format.mFramesPerPacket = 1U;
    format.mBytesPerFrame = channel_count * (UInt32)sizeof(float);
    format.mChannelsPerFrame = channel_count;
    format.mBitsPerChannel = 32U;

    return bluey_audio_bridge_push_audio_buffer_list(
        bridge,
        &buffer_list,
        frame_count,
        &format
    );
}

static int output_sample(int16_t sample, void *context) {
    BlueyOutputSink *sink = context;
    if (sink == NULL || sink->count >= sink->capacity) {
        return 0;
    }
    sink->samples[sink->count] = sample;
    sink->count += 1U;
    return 1;
}

static float read_mono_sample(const BlueyAudioPacket *packet, uint32_t frame) {
    double sum = 0.0;
    uint32_t mixed_channels = 0U;

    for (uint32_t buffer_index = 0U;
         buffer_index < packet->buffer_count;
         buffer_index++) {
        uint32_t channels = packet->buffer_channels[buffer_index];
        const uint8_t *data =
            packet->payload + packet->buffer_offsets[buffer_index];
        for (uint32_t channel = 0U; channel < channels; channel++) {
            size_t sample_index =
                (size_t)frame * (size_t)channels + (size_t)channel;
            if (packet->sample_format == BLUEY_PACKET_FORMAT_F32) {
                float value;
                memcpy(
                    &value,
                    data + sample_index * (size_t)packet->sample_size,
                    sizeof(value)
                );
                if (isfinite(value)) {
                    sum += (double)value;
                }
            } else {
                int16_t value;
                memcpy(
                    &value,
                    data + sample_index * (size_t)packet->sample_size,
                    sizeof(value)
                );
                sum += (double)value / 32768.0;
            }
            mixed_channels += 1U;
        }
    }

    if (mixed_channels == 0U) {
        return 0.0f;
    }
    double mono = sum / (double)mixed_channels;
    if (mono > 1.0) {
        mono = 1.0;
    } else if (mono < -1.0) {
        mono = -1.0;
    }
    return (float)mono;
}

BlueyAudioProcessResult bluey_audio_processor_process_next(
    BlueyAudioBridge *bridge,
    BlueyAudioProcessor *processor,
    int16_t *output,
    size_t output_capacity,
    size_t *output_count
) {
    if (output_count != NULL) {
        *output_count = 0U;
    }
    if (bridge == NULL || processor == NULL || output == NULL
        || output_capacity == 0U || output_count == NULL) {
        return BLUEY_AUDIO_PROCESS_INVALID;
    }

    unsigned long long read_sequence = atomic_load_explicit(
        &bridge->read_sequence,
        memory_order_relaxed
    );
    unsigned long long write_sequence = atomic_load_explicit(
        &bridge->write_sequence,
        memory_order_acquire
    );
    if (read_sequence == write_sequence) {
        return BLUEY_AUDIO_PROCESS_EMPTY;
    }

    size_t packet_index =
        (size_t)(read_sequence % (unsigned long long)BLUEY_AUDIO_RING_CAPACITY);
    const BlueyAudioPacket *packet = &bridge->packets[packet_index];
    BlueyAudioProcessResult result = BLUEY_AUDIO_PROCESS_OK;

    if (!processor->initialized
        || fabs(processor->source_rate - packet->sample_rate) > 0.1) {
        if (!bluey_resampler_init(&processor->resampler, packet->sample_rate)) {
            result = BLUEY_AUDIO_PROCESS_INVALID;
            goto release_packet;
        }
        processor->source_rate = packet->sample_rate;
        processor->initialized = 1;
    }

    BlueyOutputSink sink = {
        .samples = output,
        .capacity = output_capacity,
        .count = 0U,
    };
    for (uint32_t frame = 0U; frame < packet->frame_count; frame++) {
        float mono = read_mono_sample(packet, frame);
        if (!bluey_resampler_push(
                &processor->resampler,
                mono,
                output_sample,
                &sink
            )) {
            result = BLUEY_AUDIO_PROCESS_OUTPUT_FULL;
            break;
        }
    }
    *output_count = sink.count;

release_packet:
    atomic_store_explicit(
        &bridge->read_sequence,
        read_sequence + 1ULL,
        memory_order_release
    );
    return result;
}

void bluey_audio_bridge_request_stop(BlueyAudioBridge *bridge) {
    if (bridge != NULL) {
        atomic_store_explicit(&bridge->stop_requested, 1, memory_order_release);
    }
}

bool bluey_audio_bridge_stop_requested(const BlueyAudioBridge *bridge) {
    return bridge != NULL
        && atomic_load_explicit(&bridge->stop_requested, memory_order_acquire) != 0;
}

bool bluey_audio_bridge_is_empty(const BlueyAudioBridge *bridge) {
    if (bridge == NULL) {
        return true;
    }
    unsigned long long read_sequence = atomic_load_explicit(
        &bridge->read_sequence,
        memory_order_acquire
    );
    unsigned long long write_sequence = atomic_load_explicit(
        &bridge->write_sequence,
        memory_order_acquire
    );
    return read_sequence == write_sequence;
}

void bluey_audio_bridge_report_capture_failure(
    BlueyAudioBridge *bridge,
    int32_t failure_code
) {
    if (bridge == NULL || failure_code == 0) {
        return;
    }
    int expected = 0;
    (void)atomic_compare_exchange_strong_explicit(
        &bridge->capture_failure,
        &expected,
        failure_code,
        memory_order_release,
        memory_order_relaxed
    );
    bluey_audio_bridge_request_stop(bridge);
}

int32_t bluey_audio_bridge_capture_failure(const BlueyAudioBridge *bridge) {
    if (bridge == NULL) {
        return 0;
    }
    return (int32_t)atomic_load_explicit(
        &bridge->capture_failure,
        memory_order_acquire
    );
}

void bluey_audio_bridge_report_worker_failure(
    BlueyAudioBridge *bridge,
    BlueyAudioWorkerFailure failure
) {
    if (bridge == NULL || failure == BLUEY_AUDIO_WORKER_FAILURE_NONE) {
        return;
    }
    int expected = BLUEY_AUDIO_WORKER_FAILURE_NONE;
    (void)atomic_compare_exchange_strong_explicit(
        &bridge->worker_failure,
        &expected,
        (int)failure,
        memory_order_release,
        memory_order_relaxed
    );
    bluey_audio_bridge_request_stop(bridge);
}

int32_t bluey_audio_bridge_worker_failure(
    const BlueyAudioBridge *bridge
) {
    if (bridge == NULL) {
        return BLUEY_AUDIO_WORKER_FAILURE_PROCESSOR;
    }
    return (int32_t)atomic_load_explicit(
        &bridge->worker_failure,
        memory_order_acquire
    );
}

void bluey_audio_bridge_report_output_closed(BlueyAudioBridge *bridge) {
    if (bridge != NULL) {
        atomic_store_explicit(&bridge->output_closed, 1, memory_order_release);
        bluey_audio_bridge_request_stop(bridge);
    }
}

bool bluey_audio_bridge_output_closed(const BlueyAudioBridge *bridge) {
    return bridge != NULL
        && atomic_load_explicit(&bridge->output_closed, memory_order_acquire) != 0;
}

uint64_t bluey_audio_bridge_take_dropped_packets(BlueyAudioBridge *bridge) {
    if (bridge == NULL) {
        return 0U;
    }
    return (uint64_t)atomic_exchange_explicit(
        &bridge->dropped_packets,
        0ULL,
        memory_order_acq_rel
    );
}

uint64_t bluey_audio_bridge_take_invalid_packets(BlueyAudioBridge *bridge) {
    if (bridge == NULL) {
        return 0U;
    }
    return (uint64_t)atomic_exchange_explicit(
        &bridge->invalid_packets,
        0ULL,
        memory_order_acq_rel
    );
}

uint32_t bluey_audio_bridge_capacity(void) {
    return BLUEY_AUDIO_RING_CAPACITY;
}

uint32_t bluey_audio_bridge_max_packet_frames(void) {
    return BLUEY_AUDIO_MAX_PACKET_FRAMES;
}

size_t bluey_audio_bridge_max_output_samples(void) {
    return BLUEY_AUDIO_MAX_OUTPUT_SAMPLES;
}
