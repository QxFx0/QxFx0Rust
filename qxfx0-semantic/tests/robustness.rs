//! Seeded robustness gate: the proposition parser must never panic,
//! whatever bytes the input carries. Deterministic (fixed seed) so CI and
//! local runs exercise the exact same hostile corpus.

use qxfx0_semantic::PropositionParser;

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
    'а', 'Я', 'ё', 'x', 'Z', '0', '9', ' ', '\t', '\n', '\r', '.', ',', '!', '?', ';', ':', '«',
    '»', '"', '\'', '-', '(', ')', '\u{0}', '\u{7}', '\u{7f}', '😀', '\u{200d}', '́', '中', 'م',
    'ß', 'ﬁ', '%', '/', '\\', '|', '*', '_', '=', '+', '#', '@',
];

fn hostile_input(rng: &mut XorShift64) -> String {
    const LENGTHS: &[usize] = &[
        0, 1, 2, 3, 7, 16, 64, 300, 1000, 4096, 8191, 8192, 8193, 9000,
    ];
    let len = if rng.below(4) == 0 {
        LENGTHS[rng.below(LENGTHS.len())]
    } else {
        rng.below(512)
    };
    (0..len).map(|_| POOL[rng.below(POOL.len())]).collect()
}

#[test]
fn parser_never_panics_on_hostile_input() {
    let mut rng = XorShift64(0x51ab3f9d2c7e4011);
    for _ in 0..2000 {
        let input = hostile_input(&mut rng);
        let parsed = PropositionParser::parse(&input);
        let _ = format!("{parsed:?}");
    }
}

#[test]
fn parser_is_deterministic_on_hostile_input() {
    let mut rng = XorShift64(0x0ddc0ffee5eed);
    let inputs: Vec<String> = (0..200).map(|_| hostile_input(&mut rng)).collect();
    for input in &inputs {
        let first = format!("{:?}", PropositionParser::parse(input));
        let second = format!("{:?}", PropositionParser::parse(input));
        assert_eq!(first, second, "parser drifted on {input:?}");
    }
}
