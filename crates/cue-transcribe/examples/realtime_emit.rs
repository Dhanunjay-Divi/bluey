use std::time::{Duration, Instant};
fn wav(p: &str) -> Vec<i16> {
    let mut r = hound::WavReader::open(p).unwrap();
    r.samples::<i16>().map(|s| s.unwrap()).collect()
}
fn main() {
    let model = std::env::var("BLUEY_PARAKEET_MODEL_DIR").unwrap();
    let s = wav(&std::env::var("BLUEY_TEST_WAV").unwrap());
    let mut e = cue_transcribe::SttEngine::load(&model).unwrap();
    let start = Instant::now();
    let mut lastemit = start;
    for blk in s.chunks(320) {
        // 20ms chunks
        let f: Vec<f32> = blk.iter().map(|&x| x as f32 / 32768.0).collect();
        if let Ok(Some(tc)) = e.push(&f) {
            let now = Instant::now();
            println!(
                "  [{:5.2}s] +{:5.0}ms emit: {:?}",
                now.duration_since(start).as_secs_f64(),
                now.duration_since(lastemit).as_millis(),
                tc.text
            );
            lastemit = now;
        }
        std::thread::sleep(Duration::from_millis(20)); // REAL-TIME pacing
    }
    println!(
        "  (audio was {:.1}s; if emits are ~560ms apart = healthy live streaming)",
        s.len() as f64 / 16000.0
    );
}
