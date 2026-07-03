use std::time::Instant;
fn wav(path: &str) -> Vec<i16> {
    let mut r = hound::WavReader::open(path).unwrap();
    r.samples::<i16>().map(|s| s.unwrap()).collect()
}
fn run(model: &str, samples: &[i16], chunk: usize, label: &str) {
    let mut e = cue_transcribe::SttEngine::load(model).unwrap();
    let audio_s = samples.len() as f64 / 16000.0;
    let t = Instant::now();
    let mut worst = 0.0f64;
    for blk in samples.chunks(chunk) {
        let f: Vec<f32> = blk.iter().map(|&s| s as f32 / 32768.0).collect();
        let c = Instant::now();
        let _ = e.push(&f);
        let dt = c.elapsed().as_secs_f64();
        if dt > worst {
            worst = dt;
        }
    }
    let wall = t.elapsed().as_secs_f64();
    println!(
        "  {}: {}-sample chunks -> wall {:.2}s for {:.1}s audio (RTF {:.2}x), worst push {:.0}ms",
        label,
        chunk,
        wall,
        audio_s,
        wall / audio_s,
        worst * 1000.0
    );
}
fn main() {
    let model = std::env::var("BLUEY_PARAKEET_MODEL_DIR").unwrap();
    let wavf = std::env::var("BLUEY_TEST_WAV").unwrap();
    let s = wav(&wavf);
    run(&model, &s, 1600, "100ms test-style");
    run(&model, &s, 320, "20ms DAEMON-style");
}
