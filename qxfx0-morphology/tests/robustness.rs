//! Seeded robustness gate: morphology lookups must never panic on hostile
//! surfaces. Deterministic (fixed seed).

use qxfx0_morphology::{lemmatize_surface, Case, MorphologyData};

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
    'а', 'Я', 'ё', 'й', 'ъ', 'x', 'Z', '0', '9', ' ', '\t', '\n', '.', ',', '!', '?', '-', '\'',
    '«', '»', '\u{0}', '\u{7f}', '😀', '́', 'ß', 'ﬁ', 'й',
];

fn hostile_surface(rng: &mut XorShift64) -> String {
    const LENGTHS: &[usize] = &[0, 1, 2, 5, 24, 128, 1024, 8192];
    let len = if rng.below(4) == 0 {
        LENGTHS[rng.below(LENGTHS.len())]
    } else {
        rng.below(64)
    };
    (0..len).map(|_| POOL[rng.below(POOL.len())]).collect()
}

const CASES: &[Case] = &[
    Case::Nominative,
    Case::Genitive,
    Case::Dative,
    Case::Accusative,
    Case::Instrumental,
    Case::Prepositional,
];

#[test]
fn morphology_never_panics_on_hostile_surfaces() {
    let morph = MorphologyData::with_seed();
    let mut rng = XorShift64(0x6d6f727068f00d);
    for _ in 0..1500 {
        let surface = hostile_surface(&mut rng);
        for case in CASES {
            let _ = morph.to_case(*case, &surface);
        }
        let _ = morph.lemmatize(&surface);
        let _ = morph.lemmatize_any(&surface);
        let _ = lemmatize_surface(&surface);
    }
}
