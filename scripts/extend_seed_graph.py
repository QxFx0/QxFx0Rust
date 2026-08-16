#!/usr/bin/env python3
"""Extend the seed graph with the 35 graph-blocked candidate topics.

The candidate review (scripts/review_corpus_candidates.py) showed that 35 of
39 corpus candidates stamp a `graph_atom_id` that does not exist: the pack
builder copied the topic token. This script performs the editorial remedy —
real atoms with real edges — instead of nulling the ids.

Every new atom is CatConcept with the Cyrillic lemma as id/display, matching
the four candidates that already verified (воспроизводимость, выбор,
доказательство, закон — all CatConcept). CatTopic stays reserved for the
107 recognized topics backed by core-pack concepts; extending that set is a
separately reviewed catalog decision, not a graph edit. Every edge carries its Haskell curated predicate
verbatim as `ru_original`/`en_original` (provenance: haskell-corpus lines),
so verbalization quality is inherited, not synthesized. The structural
fields (rel_type, target atom, object case/text) are the editorial choices
recorded in the table below; targets are existing or newly added atoms only.

After running: copy the printed SHA-256 into SEED_GRAPH_SHA256 in
qxfx0-semantic/src/seed.rs and update the census constants (tests, README).
The script refuses to run twice (idempotence guard on the first atom).
"""

import hashlib
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
ASSET = REPO / "qxfx0-semantic/assets/seed_graph.json"

# topic -> [(rel_type, target_atom, object_case, object_text), ...]
# ru/en originals are the verbatim Haskell predicates for the same slot.
EXTENSION = {
    "аксиома": [
        ("RelDiffersFrom", "доказательство", "CaseGenitive", "доказательства"),
        ("RelSupports", "мышление", "CaseAccusative", "основание для вывода"),
    ],
    "алгоритм": [
        ("RelDetermines", "действие", "CaseAccusative", "последовательность действий"),
        ("RelGives", "результат", "CaseAccusative", "результат при корректном исполнении"),
    ],
    "беспристрастность": [
        ("RelRequires", "интерес", "CaseAccusative", "дистанцию от предмета"),
        ("RelSupports", "объективность", "CaseAccusative", "объективную оценку"),
    ],
    "благодарность": [
        ("RelExpresses", "добро", "CaseAccusative", "признание полученного добра"),
        ("RelSupports", "отношения", "CaseAccusative", "связи между людьми"),
    ],
    "верность": [
        ("RelPresupposes", "отношения", "CaseAccusative", "постоянство в отношениях"),
        ("RelVerifiedBy", "опыт", "CaseAccusative", "испытания"),
    ],
    "граница": [
        ("RelDiffersFrom", "порядок", "CaseGenitive", "внутреннего и внешнего"),
        ("RelSets", "порядок", "CaseAccusative", "условие формы"),
    ],
    "данные": [
        ("RelSupports", "знание", "CaseAccusative", "основу для суждения"),
        ("RelRequires", "интерпретация", "CaseAccusative", "интерпретации"),
    ],
    "длительность": [
        ("RelExpresses", "время", "CaseAccusative", "временное измерение существования"),
        ("RelDiffersFrom", "процесс", "CaseGenitive", "события"),
    ],
    "договор": [
        ("RelSets", "закон", "CaseAccusative", "взаимные обязательства"),
        ("RelRequires", "доверие", "CaseGenitive", "доверия к исполнению"),
    ],
    "долженствование": [
        ("RelExpresses", "мораль", "CaseAccusative", "моральное требование"),
        ("RelDiffersFrom", "желание", "CaseGenitive", "желания"),
    ],
    "дух": [
        ("RelExpresses", "сущность", "CaseAccusative", "нематериальную сущность"),
        ("RelDiffersFrom", "человек", "CaseGenitive", "тела"),
    ],
    "душа": [
        ("RelIsA", "опыт", "CaseInstrumental", "глубиной личного опыта"),
        ("RelExpresses", "жизнь", "CaseAccusative", "единство внутренней жизни"),
    ],
    "идентичность": [
        ("RelPresupposes", "время", "CaseAccusative", "преемственность во времени"),
        ("RelBuiltThrough", "самоопределение", "CaseAccusative", "самоопределение"),
    ],
    "инстинкт": [
        ("RelOrientsToward", "действие", "CaseAccusative", "действия без осознания"),
        ("RelDiffersFrom", "воля", "CaseGenitive", "воли"),
    ],
    "когерентность": [
        ("RelExpresses", "система", "CaseAccusative", "согласованность частей"),
        ("RelGives", "гармония", "CaseAccusative", "цельность"),
    ],
    "код": [
        ("RelExpresses", "информация", "CaseAccusative", "информацию в компактной форме"),
        ("RelRequires", "знак", "CaseGenitive", "декодирования"),
    ],
    "логика": [
        ("RelSets", "мышление", "CaseAccusative", "правила мышления"),
        ("RelSupports", "разум", "CaseAccusative", "последовательность рассуждений"),
    ],
    "нация": [
        ("RelTransforms", "идентичность", "CaseAccusative", "коллективную идентичность"),
        ("RelReliesOn", "культура", "CaseAccusative", "общую историю и культуру"),
    ],
    "нейрон": [
        ("RelIsA", "система", "CaseInstrumental", "единицей нервной системы"),
        ("RelTransforms", "информация", "CaseAccusative", "информацию"),
    ],
    "объективность": [
        ("RelRequires", "мнение", "CaseGenitive", "независимости от перспективы"),
        ("RelVerifiedBy", "диалог", "CaseAccusative", "межсубъектное согласие"),
    ],
    "поэзия": [
        ("RelExpresses", "красота", "CaseAccusative", "красоту через язык"),
        ("RelTransforms", "граница", "CaseAccusative", "границы выразимого"),
    ],
    "право": [
        ("RelSets", "закон", "CaseAccusative", "нормативные границы"),
        ("RelExpresses", "справедливость", "CaseAccusative", "притязание на справедливость"),
    ],
    "присутствие": [
        ("RelExpresses", "время", "CaseAccusative", "включённость в момент"),
        ("RelDiffersFrom", "покой", "CaseGenitive", "бездействия"),
    ],
    "психика": [
        ("RelExpresses", "жизнь", "CaseAccusative", "внутреннюю жизнь субъекта"),
        ("RelDiffersFrom", "человек", "CaseGenitive", "тела"),
    ],
    "революция": [
        ("RelDestroys", "традиция", "CaseAccusative", "непрерывность"),
        ("RelReveals", "будущее", "CaseAccusative", "новые горизонты"),
    ],
    "ремонт": [
        ("RelReconstructs", "система", "CaseAccusative", "функцию"),
        ("RelPresupposes", "причина", "CaseAccusative", "диагностику поломки"),
    ],
    "решимость": [
        ("RelTransforms", "сомнение", "CaseAccusative", "колебания"),
        ("RelExpresses", "воля", "CaseAccusative", "силу воли"),
    ],
    "рынок": [
        ("RelSets", "обмен", "CaseAccusative", "баланс спроса и предложения"),
        ("RelExpresses", "ценность", "CaseAccusative", "экономическую рациональность"),
    ],
    "самоопределение": [
        ("RelExpresses", "идентичность", "CaseAccusative", "идентичность"),
        ("RelSets", "граница", "CaseAccusative", "границы я"),
    ],
    "самооценка": [
        ("RelSets", "личность", "CaseAccusative", "отношение к себе"),
        ("RelCanBe", "развитие", "CaseInstrumental", "источником роста или ограничения"),
    ],
    "свидетельство": [
        ("RelSupports", "правда", "CaseAccusative", "факты"),
        ("RelRequires", "истина", "CaseGenitive", "достоверности"),
    ],
    "слушание": [
        ("RelReveals", "человек", "CaseAccusative", "доступ к другому"),
        ("RelRequires", "внимание", "CaseGenitive", "внимательности"),
    ],
    "собственность": [
        ("RelExpresses", "ценность", "CaseAccusative", "отношение к вещам"),
        ("RelSets", "граница", "CaseAccusative", "границы"),
    ],
    "становление": [
        ("RelTransformsInto", "действие", "CaseAccusative", "актуальность"),
        ("RelRelatedTo", "развитие", "CaseInstrumental", "развитием во времени"),
    ],
    "цифра": [
        ("RelExpresses", "наука", "CaseAccusative", "дискретность и точность"),
        ("RelSets", "опыт", "CaseAccusative", "опыт счёта"),
    ],
}

# Verbatim Haskell curated predicates (ru, en) per topic, prop first.
PREDICATES = {
    "аксиома": [
        ("аксиома принимается без доказательства", "an axiom is accepted without proof"),
        ("аксиома служит основанием для вывода", "an axiom serves as a foundation for inference"),
    ],
    "алгоритм": [
        ("алгоритм определяет последовательность действий", "an algorithm defines a sequence of actions"),
        ("алгоритм гарантирует результат при корректном исполнении", "an algorithm guarantees a result with correct execution"),
    ],
    "беспристрастность": [
        ("беспристрастность требует дистанции от предмета", "impartiality requires distance from the subject"),
        ("беспристрастность способствует объективной оценке", "impartiality contributes to objective evaluation"),
    ],
    "благодарность": [
        ("благодарность выражает признание полученного добра", "gratitude expresses recognition of received good"),
        ("благодарность укрепляет связи между людьми", "gratitude strengthens bonds between people"),
    ],
    "верность": [
        ("верность предполагает постоянство в отношениях", "faithfulness presupposes constancy in relationships"),
        ("верность проверяется в испытаниях", "faithfulness is tested in trials"),
    ],
    "граница": [
        ("граница различает внутреннее и внешнее", "a boundary distinguishes inner and outer"),
        ("граница устанавливает условие формы", "a boundary establishes the condition of form"),
    ],
    "данные": [
        ("данные служат основой для суждения", "data serve as the basis for judgement"),
        ("данные требуют интерпретации", "data require interpretation"),
    ],
    "длительность": [
        ("длительность выражает временное измерение существования", "duration expresses the temporal dimension of existence"),
        ("длительность отличает процесс от события", "duration distinguishes process from event"),
    ],
    "договор": [
        ("договор устанавливает взаимные обязательства", "an agreement establishes mutual obligations"),
        ("договор требует доверия к исполнению", "an agreement requires trust in execution"),
    ],
    "долженствование": [
        ("долженствование выражает моральное требование", "obligation expresses a moral demand"),
        ("долженствование отличается от желания", "obligation differs from desire"),
    ],
    "дух": [
        ("дух выражает нематериальную сущность", "spirit expresses an immaterial essence"),
        ("дух отличается от тела", "spirit differs from body"),
    ],
    "душа": [
        ("душа есть глубина личного опыта", "soul is the depth of personal experience"),
        ("душа выражает единство внутренней жизни", "soul expresses the unity of inner life"),
    ],
    "идентичность": [
        ("идентичность предполагает преемственность во времени", "identity presupposes continuity over time"),
        ("идентичность различается через самоопределение", "identity is differentiated through self-determination"),
    ],
    "инстинкт": [
        ("инстинкт направляет действия без осознания", "instinct directs actions without awareness"),
        ("инстинкт отличается от воли", "instinct differs from will"),
    ],
    "когерентность": [
        ("когерентность выражает согласованность частей", "coherence expresses the consistency of parts"),
        ("когерентность обеспечивает цельность", "coherence ensures integrity"),
    ],
    "код": [
        ("код передаёт информацию в компактной форме", "code conveys information in a compact form"),
        ("код требует декодирования", "code requires decoding"),
    ],
    "логика": [
        ("логика устанавливает правила мышления", "logic establishes the rules of thinking"),
        ("логика обеспечивает последовательность рассуждений", "logic ensures the consistency of reasoning"),
    ],
    "нация": [
        ("нация формирует коллективную идентичность", "a nation forms collective identity"),
        ("нация основана на общей истории и культуре", "a nation is based on shared history and culture"),
    ],
    "нейрон": [
        ("нейрон является основной единицей нервной системы", "a neuron is the basic unit of the nervous system"),
        ("нейрон обрабатывает и передаёт информацию", "a neuron processes and transmits information"),
    ],
    "объективность": [
        ("объективность требует независимости от перспективы", "objectivity requires independence from perspective"),
        ("объективность проверяется межсубъектным согласием", "objectivity is verified by intersubjective agreement"),
    ],
    "поэзия": [
        ("поэзия выражает красоту через язык", "poetry expresses beauty through language"),
        ("поэзия преодолевает границы выразимого", "poetry overcomes the boundaries of the expressible"),
    ],
    "право": [
        ("право устанавливает нормативные границы", "right establishes normative boundaries"),
        ("право выражает притязание на справедливость", "right expresses a claim to justice"),
    ],
    "присутствие": [
        ("присутствие выражает включённость в момент", "presence expresses inclusion in the moment"),
        ("присутствие отличается от бездействия осознанностью", "presence differs from inaction by awareness"),
    ],
    "психика": [
        ("психика выражает внутреннюю жизнь субъекта", "psyche expresses the inner life of the subject"),
        ("психика отличается от тела нематериальностью", "psyche differs from body by immateriality"),
    ],
    "революция": [
        ("революция прерывает непрерывность", "revolution interrupts continuity"),
        ("революция открывает новые горизонты", "revolution opens new horizons"),
    ],
    "ремонт": [
        ("ремонт восстанавливает функцию", "repair restores function"),
        ("ремонт предполагает диагностику поломки", "repair presupposes diagnosis of breakage"),
    ],
    "решимость": [
        ("решимость преодолевает колебания", "determination overcomes hesitation"),
        ("решимость выражает силу воли", "determination expresses the strength of will"),
    ],
    "рынок": [
        ("рынок балансирует спрос и предложение", "market balances supply and demand"),
        ("рынок выражает экономическую рациональность", "market expresses economic rationality"),
    ],
    "самоопределение": [
        ("самоопределение выражает идентичность", "self-determination expresses identity"),
        ("самоопределение формирует границы я", "self-determination forms the boundaries of the self"),
    ],
    "самооценка": [
        ("самооценка устанавливает отношение к себе", "self-assessment establishes the attitude toward oneself"),
        ("самооценка может быть источником роста или ограничения", "self-assessment can be a source of growth or limitation"),
    ],
    "свидетельство": [
        ("свидетельство подтверждает факты", "testimony confirms facts"),
        ("свидетельство требует достоверности", "testimony requires authenticity"),
    ],
    "слушание": [
        ("слушание открывает доступ к другому", "listening opens access to the other"),
        ("слушание требует внимательности", "listening requires attentiveness"),
    ],
    "собственность": [
        ("собственность выражает отношение к вещам", "ownership expresses the relationship to things"),
        ("собственность устанавливает границы", "ownership establishes boundaries"),
    ],
    "становление": [
        ("становление — это процесс перехода из потенции в актуальность", "becoming is the process of transitioning from potentiality to actuality"),
        ("становление связано с развитием во времени", "becoming is related to development over time"),
    ],
    "цифра": [
        ("цифра выражает дискретность и точность", "a digit expresses discreteness and precision"),
        ("цифра формализует опыт счёта", "a digit formalizes the experience of counting"),
    ],
}


def main():
    asset = json.loads(ASSET.read_text(encoding="utf-8"))
    if "аксиома" in asset["atoms"]:
        sys.exit("seed graph already carries the extension (аксиома present)")

    topics = sorted(EXTENSION)
    if sorted(PREDICATES) != topics:
        sys.exit("extension table and predicate table disagree")

    for topic in topics:
        asset["atoms"][topic] = {
            "id": topic,
            "display": topic,
            "category": "CatConcept",
        }

    known = set(asset["atoms"])
    for topic in topics:
        rows = EXTENSION[topic]
        preds = PREDICATES[topic]
        if len(rows) != len(preds):
            sys.exit(f"{topic}: edge/predicate count mismatch")
        for (rel_type, target, object_case, object_text), (ru, en) in zip(rows, preds):
            if target not in known:
                sys.exit(f"{topic}: target atom {target!r} does not exist")
            asset["edges"].append(
                {
                    "from": topic,
                    "to": target,
                    "rel_type": rel_type,
                    "object_case": object_case,
                    "object_text": object_text,
                    "verb_override": None,
                    "ru_original": ru,
                    "en_original": en,
                    "source": "SeedFromPredicate",
                    "topic": topic,
                    "rationale": None,
                    "counter": None,
                    "synthesis": None,
                }
            )

    # atoms serialize through a BTreeMap: keep insertion order sorted so the
    # emitted JSON stays canonical for the Rust loader and its sha pin.
    asset["atoms"] = {key: asset["atoms"][key] for key in sorted(asset["atoms"])}

    text = json.dumps(asset, ensure_ascii=False, indent=2)
    ASSET.write_text(text, encoding="utf-8", newline="\n")
    digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
    print(f"atoms: {len(asset['atoms'])} edges: {len(asset['edges'])}", file=sys.stderr)
    print(f"sha256:{digest}", file=sys.stderr)


if __name__ == "__main__":
    main()
