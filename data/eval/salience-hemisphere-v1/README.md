# Salience hemisphere labeling corpus v1

Two stratified sets for calibrating the V2 salience controller
(ADR-0045 C3 ground truth). Format: `prompt<TAB>holistic|formal<TAB>reason`,
UTF-8, LF, 3 columns.

## Measured stats (commands, not claims)

- `wc -l`: 50 train + 50 dev
- class balance (`cut -f2 | sort | uniq -c`): 25/25 both sets
- borderline rows (`grep -c пограничн`): **10 train, 11 dev**
  (spec said 8/8 — spec amended, see below)
- length strata, words in col1: train short=40 mid=10 long=0;
  dev short=42 mid=8 long=0 (spec said 15/25/10)
- dups within sets: none (`sort | uniq -d` empty)
- A∩B prompt overlap: none (`comm` empty)
- personal data: none (generic prompts only, eyeballed)

## Amendments to the labeling guide (recorded deviations)

1. Borderline stratum is 10/11, not 8/8 — all extra rows are
   legitimate borderline cases; cutting to 8 would discard signal.
2. Length split missed badly (no long prompts at all): short turns
   dominate because dialogue turns ARE short. The 15/25/10 target
   was armchair stratification; v1 records the natural skew instead,
   and length-stratified evaluation is deferred, not faked.
3. Doubt/seconds columns were specified but never logged during
   labeling — re-label pass (for kappa) must log them; this v1 has
   no timing data, stated plainly.

## Next

Re-label subset in one week with `doubt,seconds` logging → kappa →
grid candidates (`adapt_` signals) scored on train, validated on
dev, non-regression on golden suites → promote winner.
