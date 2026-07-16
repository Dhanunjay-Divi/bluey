#ifndef BLUEY_MACOS_AUDIO_BRIDGE_H
#define BLUEY_MACOS_AUDIO_BRIDGE_H

#include <AudioToolbox/AudioToolbox.h>
#include <CoreMedia/CoreMedia.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct BlueyAudioBridge BlueyAudioBridge;
typedef struct BlueyAudioProcessor BlueyAudioProcessor;

typedef enum BlueyAudioProcessResult {
    BLUEY_AUDIO_PROCESS_EMPTY = 0,
    BLUEY_AUDIO_PROCESS_OK = 1,
    BLUEY_AUDIO_PROCESS_INVALID = -1,
    BLUEY_AUDIO_PROCESS_OUTPUT_FULL = -2
} BlueyAudioProcessResult;

typedef enum BlueyAudioWorkerFailure {
    BLUEY_AUDIO_WORKER_FAILURE_NONE = 0,
    BLUEY_AUDIO_WORKER_FAILURE_PROCESSOR = 1,
    BLUEY_AUDIO_WORKER_FAILURE_STDOUT = 2,
    BLUEY_AUDIO_WORKER_FAILURE_TIMEOUT = 3
} BlueyAudioWorkerFailure;

BlueyAudioBridge *bluey_audio_bridge_create(void);
void bluey_audio_bridge_destroy(BlueyAudioBridge *bridge);

BlueyAudioProcessor *bluey_audio_processor_create(void);
void bluey_audio_processor_destroy(BlueyAudioProcessor *processor);

/**
 * Copy one Core Media sample buffer into the preallocated SPSC ring.
 *
 * This is safe for the realtime producer path: it performs no heap allocation,
 * locking, resampling, logging, or file I/O. The return value is the number of
 * fixed-size packets published to the consumer.
 */
uint32_t bluey_audio_bridge_push_sample_buffer(
    BlueyAudioBridge *bridge,
    CMSampleBufferRef sample_buffer
);

/**
 * Copy an AudioBufferList into the preallocated SPSC ring.
 *
 * Large buffers are split across fixed-size packets. The caller must ensure
 * there is exactly one producer thread for a given bridge.
 */
uint32_t bluey_audio_bridge_push_audio_buffer_list(
    BlueyAudioBridge *bridge,
    const AudioBufferList *buffer_list,
    uint32_t frame_count,
    const AudioStreamBasicDescription *format
);

/**
 * Test/support entrypoint for packed interleaved Float32 PCM.
 */
uint32_t bluey_audio_bridge_push_interleaved_f32(
    BlueyAudioBridge *bridge,
    const float *samples,
    uint32_t frame_count,
    uint32_t channel_count,
    double sample_rate
);

/**
 * Consume and convert the next ring packet on the worker thread.
 *
 * Downmixing, polyphase resampling, sanitization, and i16 conversion all occur
 * here, never on the producer callback.
 */
BlueyAudioProcessResult bluey_audio_processor_process_next(
    BlueyAudioBridge *bridge,
    BlueyAudioProcessor *processor,
    int16_t *output,
    size_t output_capacity,
    size_t *output_count
);

void bluey_audio_bridge_request_stop(BlueyAudioBridge *bridge);
bool bluey_audio_bridge_stop_requested(const BlueyAudioBridge *bridge);
bool bluey_audio_bridge_is_empty(const BlueyAudioBridge *bridge);

void bluey_audio_bridge_report_capture_failure(
    BlueyAudioBridge *bridge,
    int32_t failure_code
);
int32_t bluey_audio_bridge_capture_failure(const BlueyAudioBridge *bridge);

void bluey_audio_bridge_report_worker_failure(
    BlueyAudioBridge *bridge,
    BlueyAudioWorkerFailure failure
);
int32_t bluey_audio_bridge_worker_failure(
    const BlueyAudioBridge *bridge
);

void bluey_audio_bridge_report_output_closed(BlueyAudioBridge *bridge);
bool bluey_audio_bridge_output_closed(const BlueyAudioBridge *bridge);

uint64_t bluey_audio_bridge_take_dropped_packets(BlueyAudioBridge *bridge);
uint64_t bluey_audio_bridge_take_invalid_packets(BlueyAudioBridge *bridge);

uint32_t bluey_audio_bridge_capacity(void);
uint32_t bluey_audio_bridge_max_packet_frames(void);
size_t bluey_audio_bridge_max_output_samples(void);

#ifdef __cplusplus
}
#endif

#endif
