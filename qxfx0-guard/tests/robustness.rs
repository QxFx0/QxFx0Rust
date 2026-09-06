//! Seeded robustness gate: the guard must never panic and its finalized
//! output must stay bounded on hostile topic/render pairs. Deterministic
//! (fixed seed).

use qxfx0_guard::{ContentQualityGate, GuardConfig};

struct XorShift64(u64);

impl XorShift64 {
    fn next(&mut self) -> u64 {
        let mut x = self.0 | 1;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound.max(1) as u64) as usize
    }
}

const POOL: &[char] = &[
    'а', 'Я', 'ё', 'x', ' ', '\t', '\n', '\r', '.', ',', '!', '?', '«', '»', '{', '}', '\u{0}',
    '\u{7f}', '😀', '%', 'т', 'о', 'к', 'с', 'и', 'ч', 'н', 'ы', 'й',
];

fn hostile_text(rng: &mut XorShift64, max: usize) -> String {
    let len = rng.below(max + 1);
    (0..len).map(|_| POOL[rng.below(POOL.len())]).collect()
}

#[test]
fn guard_never_panics_and_final_output_stays_bounded() {
    let config = GuardConfig::default();
    let mut rng = XorShift64(0x9a9d1a600d);
    for _ in 0..800 {
        let topic = hostile_text(&mut rng, 200);
        let rendered = hostile_text(&mut rng, 9000);
        let history: Vec<String> = (0..rng.below(4))
            .map(|_| hostile_text(&mut rng, 300))
            .collect();
        let _ = ContentQualityGate::evaluate(&topic, &rendered);
        let _ = ContentQualityGate::post_render_safety(&rendered, &history, &config);
        let (final_text, _) =
            ContentQualityGate::finalize_output(&topic, &rendered, &history, &config);
        assert!(
            final_text.len() <= config.max_render_length + 512,
            "finalized output escaped its bound ({} bytes)",
            final_text.len()
        );
    }
}
