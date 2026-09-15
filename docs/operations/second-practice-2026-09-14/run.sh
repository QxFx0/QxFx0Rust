#!/bin/zsh
# Practice-2: long session exercising Memory recall + forgetting.
# Turns 1-8 (свобода) plant a contradiction (weakens a position to 0.35);
# turns 9-60 (filler topics) age it past the 50-turn TTL;
# turns 61-66 return to свобода to read the recall surface.
BIN=/home/liskil/QxFx0Rust/target/release/qxfx0
DB=/tmp/opencode/practice2/practice.db
OUT=/tmp/opencode/practice2
mkdir -p "$OUT"
rm -f "$DB" "$DB-wal" "$DB-shm" "$OUT/turns.log"
turn() { "$BIN" --db "$DB" --session-id "practice-recall" turn "$1" >> "$OUT/turns.log" 2>&1 || echo "TURN FAILED: $1" >> "$OUT/turns.log"; }
OPEN=( "что такое свобода?" "свобода это возможность выбора" "что такое ответственность?" "ответственность это готовность держать последствия выбора" "но ведь выбор под принуждением не свободен?" "свобода без ответственности это произвол" "я считаю что ответственность первична а свобода вторична" "нет свобода первична: без выбора нечего держать" )
FILL=( "что такое внимание?" "внимание направляет настоящее" "внимание без памяти слепо" "внимание это выбор что заметить" "что такое память?" "память хранит прошлое" "память реконструирует а не копирует" "прошлое в памяти настоящее во внимании" "что такое доверие?" "доверие это ставка на другого" "доверие уязвимо: один провал рушит годы" "проверка требует доверия к методу" "что такое истина?" )
RETURN=( "вернемся к свободе: что держится?" "свобода требует осознанности" "ответственность требует честности с собой" "осознанный выбор и есть ответственный" "свобода и ответственность две стороны выбора" "что из сказанного о свободе устояло?" )
for t in "${OPEN[@]}"; do turn "$t"; done
for i in 1 2 3 4; do for t in "${FILL[@]}"; do turn "$t"; done; done
for t in "${RETURN[@]}"; do turn "$t"; done
echo DONE
