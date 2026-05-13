# Skill: Audio DSP Patterns

## CPAL (Cross-Platform Audio)

```rust
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

let host = cpal::default_host();
let device = host.default_input_device().expect("no input device");
let config = device.default_input_config()?;

let stream = device.build_input_stream(
    &config.into(),
    move |data: &[f32], _: &cpal::InputCallbackInfo| {
        // Process audio samples
        tx.send(data.to_vec()).ok();
    },
    |err| eprintln!("audio error: {err}"),
    None,
)?;
stream.play()?;
```

## Ring Buffer (Lock-Free Audio Pipeline)

```rust
use ringbuf::HeapRb;

// Producer (audio callback) → Consumer (processing thread)
let rb = HeapRb::<f32>::new(16384); // ~1s at 16kHz
let (mut producer, mut consumer) = rb.split();

// In audio callback (real-time safe — no allocation)
producer.push_slice(samples);

// In processing thread
let mut buf = vec![0.0f32; 512];
let read = consumer.pop_slice(&mut buf);
```

## Voice Activity Detection (VAD)

### RMS Energy (Simple)
```rust
fn rms_energy(samples: &[f32]) -> f32 {
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

fn is_speech(samples: &[f32], threshold: f32) -> bool {
    rms_energy(samples) > threshold
}
```

### WebRTC VAD (via webrtc-vad crate)
```rust
use webrtc_vad::{Vad, VadMode, SampleRate};

let mut vad = Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, VadMode::Aggressive);
let is_speech = vad.is_voice_segment(&frame_i16)?;
```

## Resampling (Rubato)

```rust
use rubato::{SincFixedIn, SincInterpolationType, SincInterpolationParameters};

let params = SincInterpolationParameters {
    sinc_len: 256,
    f_cutoff: 0.95,
    interpolation: SincInterpolationType::Linear,
    oversampling_factor: 256,
    window: rubato::WindowFunction::BlackmanHarris2,
};

// 48kHz → 16kHz
let mut resampler = SincFixedIn::<f32>::new(
    16000.0 / 48000.0, // ratio
    2.0,               // max relative ratio
    params,
    1024,              // chunk size
    1,                 // channels
)?;

let output = resampler.process(&[input_samples], None)?;
```

## Audio Format Conversion

```rust
// f32 [-1.0, 1.0] → i16 (for VAD, encoding)
fn f32_to_i16(samples: &[f32]) -> Vec<i16> {
    samples.iter().map(|&s| (s * 32767.0) as i16).collect()
}

// i16 → f32 (from capture APIs)
fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|&s| s as f32 / 32768.0).collect()
}
```
