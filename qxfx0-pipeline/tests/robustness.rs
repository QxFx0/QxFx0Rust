//! Seeded robustness gate: a full turn must never panic on hostile input,
//! and its response must stay bounded. Deterministic (fixed seed).

use qxfx0_pipeline::{process_turn_with_options, TurnInput, TurnOptions};
use qxfx0_types::system_state::SystemState;

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
    'а', 'Я', 'ё', 'с', 'в', 'о', 'б', 'д', 'x', 'Z', '0', ' ', '\t', '\n', '\r', '.', ',', '!',
    '?', '«', '»', '"', '-', '(', ')', '\u{0}', '\u{7}', '\u{7f}', '😀', '\u{200d}', '́', '中', '%',
    '_', '*', '#', '@',
];

fn hostile_text(rng: &mut XorShift64) -> String {
    const LENGTHS: &[usize] = &[0, 1, 2, 9, 64, 500, 8191, 8192, 8193, 12000];
    let len = if rng.below(4) == 0 {
        LENGTHS[rng.below(LENGTHS.len())]
    } else {
        rng.below(700)
    };
    (0..len).map(|_| POOL[rng.below(POOL.len())]).collect()
}

#[test]
fn hostile_turns_never_panic_and_stay_bounded() {
    let mut rng = XorShift64(0x7e5741ab1e55);
    for turn in 0..300 {
        let session_id = format!("robust-{turn}");
        let mut state = SystemState {
            session_id: session_id.clone(),
            ..SystemState::default()
        };
        let input = TurnInput {
            session_id,
            raw_text: hostile_text(&mut rng),
        };
        let output = process_turn_with_options(&input, &mut state, TurnOptions::new());
        assert!(
            output.response.len() < 20_000,
            "turn {turn} escaped its response bound ({} bytes)",
            output.response.len()
        );
    }
}
