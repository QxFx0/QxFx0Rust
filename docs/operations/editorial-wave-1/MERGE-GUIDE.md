# Волна-1: инструкция по merge (только человек)

Вердикты зафиксированы в `REVIEW.md`: #1–#3 admit с вашими
формулировками, #4 без вердикта. Этот файл — как перенести admitted
в pack-источники и проверить. Машина дальше не идёт.

## Шаг 0. Закройте вердикты

1. В `REVIEW.md` запишите основания для #1–#3 (колонка пуста).
2. Решите #4 (жизнь): admit / refuse с причиной / defer с условием.
   Без вердикта — не мержить.

## Шаг 1. Новые факты в authority-пак

Admitted — это новые курируемые тезисы. Каждый нужно оформить как
факт в `data/packs/philosophy-core-v1/facts.json` (291 факт сейчас),
по образцу существующей записи:

- `id`: `fact.<предикат>` (латиница, как `fact.freedom_choice`);
- `subject` / `object`: `concept.<атом>` — оба атома должны существовать
  в `concepts.json` того же пака, иначе сначала добавьте концепты;
- `relation`: один из типов, разрешённых в `relations.json`
  (для identity-утверждений волны-1 проверьте наличие нужного типа —
  нет нужного, не выдумывайте: это refuse/defer, не повод расширять
  модель ради волны);
- `kind`: `interpretive_claim` для толковательных (как все три);
- `confidence_basis_points`: честная оценка (образец: 9000);
- `source_pack`: `philosophy-core-v1`, `source_ref`:
  `predicate:<предикат>`, `status`: `curated`.

`rendered_ru` из вердикта — это ваша редакторская поверхность, она
живёт в вердикте; в пак ложится каноническая тройка
(subject/relation/object), которая её несёт.

## Шаг 2. Пак-получатель

Три факта — три разные темы (власть/доверие/долг), готового общего
пака нет. Варианты на ваше решение:

- а) расширить `agency-responsibility-v1` (долг и доверие ложатся,
  власть — на ваше усмотрение);
- б) новый тематический пак через `SPECS` в
  `scripts/build_thematic_packs.py` (образец записей и relations —
  в словаре `SPECS`; скрипт сам считает thesis/evidence дайджесты,
  манифесты и перестроит каталог).

Не смешивайте: один вердикт — одно место. Запишите выбор в `REVIEW.md`.

## Шаг 3. Пересборка и контрольные суммы

Паки вшиты в бинарь (`include_bytes`), манифесты — sha256 байтов
файлов. После правок:

```sh
# если правили philosophy-core-v1 вручную — пересчитайте его manifest:
python3 -c "
import hashlib, json, pathlib
p = pathlib.Path('data/packs/philosophy-core-v1')
m = json.loads((p/'manifest.json').read_text())
m['files'] = {n: hashlib.sha256((p/n).read_bytes()).hexdigest() for n in sorted(m['files'])}
(p/'manifest.json').write_text(json.dumps(m, ensure_ascii=False, indent=2) + '\n')
"
# тематические паки и каталог — только скриптом, не руками:
python3 scripts/build_thematic_packs.py
```

## Шаг 4. Гейты (все, без послаблений)

```sh
export PATH="/home/liskil/.rustup/toolchains/1.93.1-x86_64-unknown-linux-gnu/bin:$PATH"
cargo build --locked --workspace --release
cargo test --locked --workspace --all-targets   # парсит все паки, уронит битые
python3 scripts/generate_census.py --binary target/release/qxfx0  # регенерировать!
python3 scripts/generate_census.py --check --binary target/release/qxfx0
./target/release/qxfx0 --db /tmp/opencode/merge-check.db doctor
```

Census обязан измениться (счётчики Knowledge pack / FactRegistry
растут) — коммитить `data/census.json` вместе с паками, иначе CI
упадёт на `--check`. `doctor` — Status: OK.

## Шаг 5. Фиксация разбора

Допишите в `REVIEW.md`: что вошло (файлы, id фактов), что отказано
и почему, что отложено и при каком условии. Закоммитьте паки +
census + REVIEW одним коммитом и запушьте.

## Запрещено

- Править `overlay-2aa2a46b.json` (свидетельство машины, см. урок
  в `REVIEW.md`).
- Расширять `relations.json` (модель отношений) ради одной волны.
- Мержить #4 без вердикта.
