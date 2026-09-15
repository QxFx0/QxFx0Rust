#!/bin/zsh
# Engagement-fix check: the 8 opening turns of practice-2.
# Before the fix turn 8 engaged but never contradicted; after the fix
# the leading «нет» must record a contradiction and weaken a side.
BIN=/home/liskil/QxFx0Rust/target/release/qxfx0
DB=/tmp/opencode/engage/engage.db
mkdir -p /tmp/opencode/engage
rm -f "$DB" "$DB-wal" "$DB-shm"
OPEN=( "что такое свобода?" "свобода это возможность выбора" "что такое ответственность?" "ответственность это готовность держать последствия выбора" "но ведь выбор под принуждением не свободен?" "свобода без ответственности это произвол" "я считаю что ответственность первична а свобода вторична" "нет свобода первична: без выбора нечего держать" )
for t in "${OPEN[@]}"; do "$BIN" --db "$DB" --session-id "practice-engage" turn "$t" > /dev/null 2>&1 || echo "TURN FAILED: $t"; done
echo DONE
