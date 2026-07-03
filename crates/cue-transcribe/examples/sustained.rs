use std::time::{Duration, Instant};
fn wav(p: &str) -> Vec<i16> {
    let mut r = hound::WavReader::open(p).unwrap();
    r.samples::<i16>().map(|s| s.unwrap()).collect()
}
fn main() {
    let model = std::env::var("BLUEY_PARAKEET_MODEL_DIR").unwrap();
    let base = wav(&std::env::var("BLUEY_TEST_WAV").unwrap());
    // loop the clip to ~30s of continuous audio
    let mut s = Vec::new();
    while s.len() < 16000 * 30 {
        s.extend_from_slice(&base);
    }
    let chunk: usize = std::env::var("CHUNK")
        .unwrap_or("320".into())
        .parse()
        .unwrap();
    let mut e = cue_transcribe::SttEngine::load(&model).unwrap();
    let start = Instant::now();
    let mut n = 0u64;
    let mut worst = 0.0f64;
    let mut lastpush = start;
    for blk in s.chunks(chunk) {
        let f: Vec<f32> = blk.iter().map(|&x| x as f32 / 32768.0).collect();
        let c = Instant::now();
        let _ = e.push(&f);
        let dt = c.elapsed().as_secs_f64();
        n += 1;
        if dt > worst {
            worst = dt;
        }
        // report push-time every 5s of audio to see if it GROWS
        if start.elapsed().as_secs_f64() - lastpush.duration_since(start).as_secs_f64() > 5.0 {
            println!(
                "  at {:.0}s audio: this push took {:.0}ms (worst so far {:.0}ms)",
                n as f64 * chunk as f64 / 16000.0,
                dt * 1000.0,
                worst * 1000.0
            );
            lastpush = Instant::now();
        }
    }
    let wall = start.elapsed().as_secs_f64();
    println!(
        "  CHUNK={} : 30s audio processed in {:.1}s (RTF {:.2}x), worst push {:.0}ms",
        chunk,
        wall,
        wall / 30.0,
        worst * 1000.0
    );
}
