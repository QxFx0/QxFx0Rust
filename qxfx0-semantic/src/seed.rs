use qxfx0_types::atom::PathProof;
use qxfx0_types::*;

/// 107 covered philosophical topics.
pub const COVERED_TOPICS: &[&str] = &[
    "свобода",
    "произвол",
    "ответственность",
    "истина",
    "мнение",
    "память",
    "воспоминание",
    "помнить",
    "сознание",
    "самосознание",
    "вера",
    "красота",
    "долг",
    "доверие",
    "страх",
    "надежда",
    "справедливость",
    "время",
    "разум",
    "бытие",
    "история",
    "язык",
    "воля",
    "смерть",
    "одиночество",
    "любовь",
    "труд",
    "покой",
    "власть",
    "правда",
    "молчание",
    // New domains:
    "знание", "понимание", "сомнение", "интуиция",
    "добро", "зло", "совесть", "поступок",
    "сущность", "существование", "необходимость",
    "мышление", "воображение", "желание",
    "значение", "интерпретация", "коммуникация",
    "культура", "творчество", "мудрость",
    "прогресс", "искусство", "гармония",
    // Bridge atoms:
    "человек", "жизнь", "смысл", "счастье",
    "страдание", "природа", "игра",
    // Everyday:
    "работа", "деньги", "здоровье", "дружба",
    "семья", "образование", "музыка", "наука",
    "технология", "дом", "путешествие",
    // Psychology, politics, economics:
    "личность", "мотивация", "стресс", "развитие",
    "государство", "демократия", "равенство", "права",
    "конфликт", "ресурс", "ценность", "обмен",
    // Relationships, career, modern life:
    "отношения", "уважение", "ревность", "привязанность",
    "успех", "талант", "дисциплина", "призвание",
    "информация", "внимание", "скорость",
    // Emotions, ethics, systems, communication:
    "радость", "грусть", "гнев", "спокойствие",
    "мораль", "этика", "система", "метод",
    "процесс", "результат", "диалог", "спор",
];

/// Seed the AtomGraph with core philosophical relations.
pub fn seed_graph() -> AtomGraph {
    let mut graph = AtomGraph::new();

    for topic in COVERED_TOPICS {
        let id = AtomId::new(topic.to_string());
        graph.atoms.insert(
            id.clone(),
            Atom {
                id: id.clone(),
                display: topic.to_string(),
                category: AtomCategory::CatTopic,
            },
        );
    }

    // Non-topic concept atoms referenced as edge targets but not in COVERED_TOPICS.
    // add_relation does not auto-create atoms, so we must insert them here.
    let concept_atoms = &[
        "выбор",
        "принуждение",
        "реальность",
        "воспроизводимость",
        "самоотчёт",
        "последствия",
        "будущее",
        // Epistemology
        "опыт", "доказательство", "убеждение", "заблуждение",
        // Ethics
        "добродетель", "честь",
        // Metaphysics
        "возможность", "случайность", "причина", "следствие",
        // Mind
        "восприятие", "эмоция",
        // Language
        "знак", "символ",
        // Social
        "общество", "человек", "традиция", "закон", "порядок", "хаос",
        // Aesthetics
        "прекрасное",
        // Bridge
        "чувство",
        // Everyday
        "познание",
        // New domains
        "цель", "интерес",
        "права", "действие",
        "качество", "еда", "сон",
    ];
    for concept in concept_atoms {
        let id = AtomId::new(concept.to_string());
        graph.atoms.entry(id.clone()).or_insert(Atom {
            id: id.clone(),
            display: concept.to_string(),
            category: AtomCategory::CatConcept,
        });
    }

    let rel = |from: &str,
               to: &str,
               rt: RelationType,
               case: ObjectCase,
               obj: &str,
               ru: &str,
               en: &str,
               topic: &str,
               rationale: Option<&str>,
               counter: Option<&str>,
               synthesis: Option<&str>| {
        Relation {
            from: AtomId::new(from.to_string()),
            to: AtomId::new(to.to_string()),
            rel_type: rt,
            object_case: case,
            object_text: obj.to_string(),
            verb_override: None,
            ru_original: ru.to_string(),
            en_original: en.to_string(),
            source: RelationSource::SeedFromPredicate,
            topic: topic.to_string(),
            rationale: rationale.map(String::from),
            counter: counter.map(String::from),
            synthesis: synthesis.map(String::from),
        }
    };

    graph.add_relation(rel(
        "свобода",
        "выбор",
        RelationType::RelPresupposes,
        ObjectCase::CaseAccusative,
        "возможность выбора",
        "свобода предполагает возможность выбора",
        "freedom presupposes the possibility of choice",
        "свобода",
        Some("без выбора действие не отличается от рефлекса"),
        Some("не любой выбор свободен: выбор под принуждением не делает действие свободным"),
        Some("свобода требует не только возможности, но и осознанности выбора"),
    ));

    graph.add_relation(rel(
        "свобода",
        "ответственность",
        RelationType::RelLimitedBy,
        ObjectCase::CaseInstrumental,
        "ответственностью",
        "свобода ограничена ответственностью",
        "freedom is limited by responsibility",
        "свобода",
        Some("без ответственности свобода превращается в произвол"),
        Some("не любое ограничение убивает свободу — только произвольное"),
        Some("ответственность не враг свободы, а условие её осмысленности"),
    ));

    graph.add_relation(rel(
        "свобода",
        "принуждение",
        RelationType::RelDetermines,
        ObjectCase::CaseAccusative,
        "отсутствие принуждения",
        "свобода определяет отсутствие принуждения",
        "freedom determines the absence of coercion",
        "свобода",
        None,
        None,
        None,
    ));

    graph.add_relation(rel(
        "свобода",
        "сознание",
        RelationType::RelRequires,
        ObjectCase::CaseAccusative,
        "сознание",
        "свобода требует сознания",
        "freedom requires consciousness",
        "свобода",
        None,
        None,
        None,
    ));

    graph.add_relation(rel(
        "свобода",
        "истина",
        RelationType::RelContrastsWith,
        ObjectCase::CaseAccusative,
        "истина",
        "свобода контрастирует с истиной",
        "freedom contrasts with truth",
        "свобода",
        None,
        None,
        None,
    ));

    graph.add_relation(rel(
        "истина", "воспроизводимость", RelationType::RelVerifiedBy, ObjectCase::CaseInstrumental,
        "воспроизводимостью",         "истина проверяется через воспроизводимость", "truth is verified through reproducibility", "истина",
        Some("единичное совпадение может быть случайностью, а повторяемое — закономерностью"),
        Some("воспроизводимость не гарантирует истину — она лишь отсеивает то, что точно ею не является"),
        None,
    ));

    graph.add_relation(rel(
        "истина",
        "реальность",
        RelationType::RelClaims,
        ObjectCase::CaseAccusative,
        "соответствие реальности",
        "истина претендует на соответствие реальности",
        "truth claims correspondence to reality",
        "истина",
        None,
        None,
        None,
    ));

    graph.add_relation(rel(
        "сознание",
        "самоотчёт",
        RelationType::RelIncludes,
        ObjectCase::CaseAccusative,
        "способность к самоотчёту",
        "сознание включает способность к самоотчёту",
        "consciousness includes the capacity for self-report",
        "сознание",
        Some(
            "существо, не способное сказать «я чувствую это», может реагировать — но не осознавать",
        ),
        None,
        None,
    ));

    graph.add_relation(rel(
        "сознание",
        "разум",
        RelationType::RelContrastsWith,
        ObjectCase::CaseAccusative,
        "разум",
        "сознание контрастирует с разумом",
        "consciousness contrasts with reason",
        "сознание",
        None,
        None,
        None,
    ));

    graph.add_relation(rel(
        "ответственность",
        "долг",
        RelationType::RelRequires,
        ObjectCase::CaseAccusative,
        "долг",
        "ответственность требует долга",
        "responsibility requires duty",
        "ответственность",
        None,
        None,
        None,
    ));

    graph.add_relation(rel(
        "ответственность", "последствия", RelationType::RelRequires, ObjectCase::CaseAccusative,
        "осознания последствий",         "ответственность требует осознания последствий", "responsibility requires awareness of consequences", "ответственность",
        Some("нельзя отвечать за то, чего не понимаешь: ответственность без осознания — имитация"),
        Some("осознание последствий не гарантирует правильного выбора — оно лишь исключает неведение как оправдание"),
        None,
    ));

    graph.add_relation(rel("память","сознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"сознания","память требует сознания","memory requires consciousness","память",Some("без сознания нет того, кто помнит"),None,None));
    graph.add_relation(rel("память","воспоминание",RelationType::RelIncludes,ObjectCase::CaseAccusative,"воспоминание","память включает воспоминание","memory includes recollection","память",Some("воспоминание — акт обращения к памяти"),None,None));
    graph.add_relation(rel("воспоминание","время",RelationType::RelRequires,ObjectCase::CaseGenitive,"времени","воспоминание требует времени","recollection requires time","воспоминание",Some("воспоминание разворачивается во времени"),None,None));
    // Memory cluster: помнить (verb) connects to память (noun) and воспоминание (act)
    graph.add_relation(rel("помнить","память",RelationType::RelRequires,ObjectCase::CaseGenitive,"памяти","помнить требует памяти","to remember requires memory","помнить",Some("помнить — активировать память, без неё нечего активировать"),None,None));
    graph.add_relation(rel("помнить","воспоминание",RelationType::RelEvokes,ObjectCase::CaseAccusative,"воспоминание","помнить вызывает воспоминание","to remember evokes recollection","помнить",Some("акт памяти вызывает конкретное воспоминание"),None,None));
    graph.add_relation(rel("помнить","сознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"сознания","помнить требует сознания","to remember requires consciousness","помнить",Some("помнить может лишь тот, кто осознаёт"),None,None));
    graph.add_relation(rel("помнить","время",RelationType::RelDependsOn,ObjectCase::CaseGenitive,"времени","помнить зависит от времени","to remember depends on time","помнить",Some("память связывает прошлое с настоящим через время"),None,None));
    graph.add_relation(rel("помнить","самосознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"самосознания","помнить требует самосознания","to remember requires self-consciousness","помнить",Some("помнить себя — основа самосознания"),None,None));
    // Enrich память cluster: connect to бытие, язык, история
    graph.add_relation(rel("память","бытие",RelationType::RelStructures,ObjectCase::CaseAccusative,"бытие","память структурирует бытие","memory structures being","память",Some("память придаёт бытию непрерывность"),None,None));
    graph.add_relation(rel("память","язык",RelationType::RelExpresses,ObjectCase::CaseInstrumental,"языком","память выражается через язык","memory is expressed through language","память",Some("память передаётся через язык"),None,None));
    graph.add_relation(rel("память","история",RelationType::RelPreserves,ObjectCase::CaseAccusative,"историю","память сохраняет историю","memory preserves history","память",Some("история — коллективная память"),None,None));
    // Enrich воспоминание cluster: connect to сознание and reconstruct память
    graph.add_relation(rel("воспоминание","сознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"сознания","воспоминание требует сознания","recollection requires consciousness","воспоминание",Some("воспоминание — акт сознания, обращённый к прошлому"),None,None));
    graph.add_relation(rel("воспоминание","память",RelationType::RelReconstructs,ObjectCase::CaseAccusative,"память","воспоминание реконструирует память","recollection reconstructs memory","воспоминание",Some("воспоминание не копирует память, а реконструирует её"),None,None));
    graph.add_relation(rel("самосознание","сознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"сознания","самосознание требует сознания","self-consciousness requires consciousness","самосознание",Some("самосознание — это сознание, обращённое на себя"),None,None));
    graph.add_relation(rel("вера","доверие",RelationType::RelRequires,ObjectCase::CaseGenitive,"доверия","вера требует доверия","faith requires trust","вера",Some("вера невозможна без доверия к источнику"),None,None));
    graph.add_relation(rel("красота","истина",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"истиной","красота связана с истиной","beauty is related to truth","красота",Some("красота и истина пересекаются в гармонии"),None,None));
    graph.add_relation(rel("долг","ответственность",RelationType::RelRequires,ObjectCase::CaseGenitive,"ответственности","долг требует ответственности","duty requires responsibility","долг",Some("долг без ответственности пуст"),None,None));
    graph.add_relation(rel("доверие","вера",RelationType::RelSupports,ObjectCase::CaseInstrumental,"верой","доверие поддерживается верой","trust is supported by faith","доверие",None,None,None));
    graph.add_relation(rel("страх","смерть",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смертью","страх связан со смертью","fear is related to death","страх",Some("страх часто коренится в конечности"),None,None));
    graph.add_relation(rel("надежда","будущее",RelationType::RelOrientsToward,ObjectCase::CaseAccusative,"будущее","надежда ориентирована на будущее","hope is oriented toward the future","надежда",None,None,None));
    graph.add_relation(rel("справедливость","ответственность",RelationType::RelRequires,ObjectCase::CaseGenitive,"ответственности","справедливость требует ответственности","justice requires responsibility","справедливость",Some("справедливость невозможна без ответственности"),None,None));
    graph.add_relation(rel("время","бытие",RelationType::RelStructures,ObjectCase::CaseAccusative,"бытие","время структурирует бытие","time structures being","время",None,None,None));
    graph.add_relation(rel("разум","сознание",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"сознанием","разум связан с сознанием","reason is related to consciousness","разум",None,None,None));
    graph.add_relation(rel("бытие","время",RelationType::RelRequires,ObjectCase::CaseGenitive,"времени","бытие требует времени","being requires time","бытие",None,None,None));
    graph.add_relation(rel("история","время",RelationType::RelRequires,ObjectCase::CaseGenitive,"времени","история требует времени","history requires time","история",None,None,None));
    graph.add_relation(rel("язык","сознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"сознания","язык требует сознания","language requires consciousness","язык",Some("язык — способ выразить сознание"),None,None));
    graph.add_relation(rel("воля","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","воля требует свободы","will requires freedom","воля",None,None,None));
    graph.add_relation(rel("смерть","время",RelationType::RelLimitedBy,ObjectCase::CaseInstrumental,"временем","смерть ограничена временем","death is limited by time","смерть",None,None,None));
    graph.add_relation(rel("одиночество","самосознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"самосознания","одиночество требует самосознания","loneliness requires self-consciousness","одиночество",Some("одиночество осознаётся лишь тем, кто имеет самосознание"),None,None));
    graph.add_relation(rel("любовь","доверие",RelationType::RelRequires,ObjectCase::CaseGenitive,"доверия","любовь требует доверия","love requires trust","любовь",None,None,None));
    graph.add_relation(rel("труд","долг",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"долгом","труд связан с долгом","labor is related to duty","труд",None,None,None));
    graph.add_relation(rel("покой","смерть",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смертью","покой связан со смертью","peace is related to death","покой",None,None,None));
    graph.add_relation(rel("власть","ответственность",RelationType::RelRequires,ObjectCase::CaseGenitive,"ответственности","власть требует ответственности","power requires responsibility","власть",Some("власть без ответственности — тирания"),None,None));
    graph.add_relation(rel("правда","истина",RelationType::RelMeans,ObjectCase::CaseAccusative,"истину","правда означает истину","pravda means truth","правда",None,None,None));
    graph.add_relation(rel("молчание","язык",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"языком","молчание контрастирует с языком","silence contrasts with language","молчание",Some("молчание — оборотная сторона языка"),None,None));
    graph.add_relation(rel("произвол","свобода",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"свободой","произвол контрастирует со свободой","arbitrariness contrasts with freedom","произвол",Some("произвол — это свобода без ответственности"),None,None));
    graph.add_relation(rel("мнение","истина",RelationType::RelDiffersFrom,ObjectCase::CaseGenitive,"истины","мнение отличается от истины","opinion differs from truth","мнение",Some("мнение может быть ошибочным, истина претендует на соответствие реальности"),None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Epistemology — knowledge, understanding, doubt, evidence
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("знание","истина",RelationType::RelRequires,ObjectCase::CaseGenitive,"истины","знание требует истины","knowledge requires truth","знание",Some("знание без истины — пустая вера"),None,None));
    graph.add_relation(rel("знание","опыт",RelationType::RelDependsOn,ObjectCase::CaseAccusative,"опыт","знание опирается на опыт","knowledge relies on experience","знание",None,None,None));
    graph.add_relation(rel("знание","доказательство",RelationType::RelVerifiedBy,ObjectCase::CaseAccusative,"доказательство","знание проверяется доказательством","knowledge is verified by proof","знание",None,None,None));
    graph.add_relation(rel("понимание","знание",RelationType::RelPresupposes,ObjectCase::CaseAccusative,"знание","понимание предполагает знание","understanding presupposes knowledge","понимание",Some("нельзя понять то, чего не знаешь"),None,None));
    graph.add_relation(rel("понимание","сознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"сознания","понимание требует сознания","understanding requires consciousness","понимание",None,None,None));
    graph.add_relation(rel("сомнение","истина",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"истиной","сомнение контрастирует с истиной","doubt contrasts with truth","сомнение",Some("сомнение — двигатель познания, но не знание"),None,None));
    graph.add_relation(rel("сомнение","вера",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"верой","сомнение контрастирует с верой","doubt contrasts with faith","сомнение",None,None,None));
    graph.add_relation(rel("убеждение","доказательство",RelationType::RelRequires,ObjectCase::CaseGenitive,"доказательства","убеждение требует доказательства","conviction requires proof","убеждение",None,None,None));
    graph.add_relation(rel("убеждение","вера",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"верой","убеждение связано с верой","conviction is related to faith","убеждение",None,None,None));
    graph.add_relation(rel("доказательство","истина",RelationType::RelSignals,ObjectCase::CasePrepositional,"истине","доказательство указывает на истину","proof points to truth","доказательство",None,None,None));
    graph.add_relation(rel("опыт","знание",RelationType::RelSupports,ObjectCase::CaseAccusative,"знание","опыт подкрепляет знание","experience supports knowledge","опыт",None,None,None));
    graph.add_relation(rel("опыт","время",RelationType::RelRequires,ObjectCase::CaseGenitive,"времени","опыт требует времени","experience requires time","опыт",None,None,None));
    graph.add_relation(rel("интуиция","знание",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"знанием","интуиция связана со знанием","intuition is related to knowledge","интуиция",Some("интуиция — знание без доказательства"),None,None));
    graph.add_relation(rel("интуиция","разум",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"разумом","интуиция контрастирует с разумом","intuition contrasts with reason","интуиция",None,None,None));
    graph.add_relation(rel("заблуждение","истина",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"истиной","заблуждение контрастирует с истиной","delusion contrasts with truth","заблуждение",None,None,None));
    graph.add_relation(rel("заблуждение","мнение",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"мнением","заблуждение связано с мнением","delusion is related to opinion","заблуждение",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Ethics — virtue, goodness, conscience, deed
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("добро","зло",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"злом","добро контрастирует со злом","good contrasts with evil","добро",None,None,None));
    graph.add_relation(rel("добро","справедливость",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"справедливостью","добро связано со справедливостью","good is related to justice","добро",None,None,None));
    graph.add_relation(rel("зло","свобода",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"свободой","зло связано со свободой","evil is related to freedom","зло",Some("зло возможно только благодаря свободе выбора"),None,None));
    graph.add_relation(rel("добродетель","долг",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"долгом","добродетель связана с долгом","virtue is related to duty","добродетель",None,None,None));
    graph.add_relation(rel("добродетель","сознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"сознания","добродетель требует сознания","virtue requires consciousness","добродетель",None,None,None));
    graph.add_relation(rel("честь","долг",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"долгом","честь связана с долгом","honor is related to duty","честь",None,None,None));
    graph.add_relation(rel("честь","правда",RelationType::RelRequires,ObjectCase::CaseGenitive,"правды","честь требует правды","honor requires truth","честь",None,None,None));
    graph.add_relation(rel("совесть","ответственность",RelationType::RelSupports,ObjectCase::CaseAccusative,"ответственность","совесть поддерживает ответственность","conscience supports responsibility","совесть",None,None,None));
    graph.add_relation(rel("совесть","добро",RelationType::RelDirectedAt,ObjectCase::CaseAccusative,"добро","совесть направлена на добро","conscience is directed at good","совесть",None,None,None));
    graph.add_relation(rel("поступок","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","поступок требует свободы","deed requires freedom","поступок",None,None,None));
    graph.add_relation(rel("поступок","ответственность",RelationType::RelPresupposes,ObjectCase::CaseAccusative,"ответственность","поступок предполагает ответственность","deed presupposes responsibility","поступок",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Metaphysics — essence, existence, possibility, necessity, cause
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("сущность","бытие",RelationType::RelDetermines,ObjectCase::CaseAccusative,"бытие","сущность определяет бытие","essence determines being","сущность",None,None,None));
    graph.add_relation(rel("сущность","истина",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"истиной","сущность связана с истиной","essence is related to truth","сущность",None,None,None));
    graph.add_relation(rel("существование","время",RelationType::RelLimitedBy,ObjectCase::CaseInstrumental,"временем","существование ограничено временем","existence is limited by time","существование",None,None,None));
    graph.add_relation(rel("существование","сознание",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"сознанием","существование связано с сознанием","existence is related to consciousness","существование",None,None,None));
    graph.add_relation(rel("возможность","свобода",RelationType::RelPresupposes,ObjectCase::CaseAccusative,"свободу","возможность предполагает свободу","possibility presupposes freedom","возможность",Some("свобода — это пространство возможностей"),None,None));
    graph.add_relation(rel("возможность","выбор",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"выбором","возможность связана с выбором","possibility is related to choice","возможность",None,None,None));
    graph.add_relation(rel("необходимость","свобода",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"свободой","необходимость контрастирует со свободой","necessity contrasts with freedom","необходимость",None,None,None));
    graph.add_relation(rel("необходимость","закон",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"законом","необходимость связана с законом","necessity is related to law","необходимость",None,None,None));
    graph.add_relation(rel("случайность","необходимость",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"необходимостью","случайность контрастирует с необходимостью","chance contrasts with necessity","случайность",None,None,None));
    graph.add_relation(rel("причина","следствие",RelationType::RelDetermines,ObjectCase::CaseAccusative,"следствие","причина определяет следствие","cause determines effect","причина",None,None,None));
    graph.add_relation(rel("причина","время",RelationType::RelDependsOn,ObjectCase::CaseAccusative,"время","причина зависит от времени","cause depends on time","причина",Some("причина всегда предшествует следствию во времени"),None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Mind — perception, imagination, thinking, emotion, desire
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("восприятие","сознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"сознания","восприятие требует сознания","perception requires consciousness","восприятие",None,None,None));
    graph.add_relation(rel("восприятие","реальность",RelationType::RelDirectedAt,ObjectCase::CaseAccusative,"реальность","восприятие направлено на реальность","perception is directed at reality","восприятие",None,None,None));
    graph.add_relation(rel("воображение","восприятие",RelationType::RelDiffersFrom,ObjectCase::CaseGenitive,"восприятия","воображение отличается от восприятия","imagination differs from perception","воображение",Some("восприятие дано, воображение создано"),None,None));
    graph.add_relation(rel("воображение","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","воображение требует свободы","imagination requires freedom","воображение",None,None,None));
    graph.add_relation(rel("мышление","разум",RelationType::RelIncludes,ObjectCase::CaseAccusative,"разум","мышление включает разум","thinking includes reason","мышление",None,None,None));
    graph.add_relation(rel("мышление","язык",RelationType::RelDependsOn,ObjectCase::CaseAccusative,"язык","мышление опирается на язык","thinking relies on language","мышление",Some("мысль обретает форму в языке"),None,None));
    graph.add_relation(rel("эмоция","сознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"сознания","эмоция требует сознания","emotion requires consciousness","эмоция",None,None,None));
    graph.add_relation(rel("эмоция","разум",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"разумом","эмоция контрастирует с разумом","emotion contrasts with reason","эмоция",None,None,None));
    graph.add_relation(rel("желание","воля",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"волей","желание связано с волей","desire is related to will","желание",None,None,None));
    graph.add_relation(rel("желание","свобода",RelationType::RelPresupposes,ObjectCase::CaseAccusative,"свободу","желание предполагает свободу","desire presupposes freedom","желание",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Language & Meaning — sign, symbol, communication, interpretation
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("знак","язык",RelationType::RelIncludes,ObjectCase::CaseAccusative,"язык","язык состоит из знаков","language consists of signs","язык",None,None,None));
    graph.add_relation(rel("знак","значение",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"значением","знак связан со значением","sign is related to meaning","знак",None,None,None));
    graph.add_relation(rel("значение","интерпретация",RelationType::RelRequires,ObjectCase::CaseGenitive,"интерпретации","значение требует интерпретации","meaning requires interpretation","значение",None,None,None));
    graph.add_relation(rel("значение","сознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"сознания","значение требует сознания","meaning requires consciousness","значение",Some("значение существует только для сознания"),None,None));
    graph.add_relation(rel("символ","знак",RelationType::RelIncludes,ObjectCase::CaseAccusative,"знак","символ включает знак","symbol includes sign","символ",None,None,None));
    graph.add_relation(rel("символ","значение",RelationType::RelExpresses,ObjectCase::CaseAccusative,"значение","символ выражает значение","symbol expresses meaning","символ",None,None,None));
    graph.add_relation(rel("коммуникация","язык",RelationType::RelRequires,ObjectCase::CaseGenitive,"языка","коммуникация требует языка","communication requires language","коммуникация",None,None,None));
    graph.add_relation(rel("коммуникация","понимание",RelationType::RelDirectedAt,ObjectCase::CaseAccusative,"понимание","коммуникация направлена на понимание","communication is directed at understanding","коммуникация",None,None,None));
    graph.add_relation(rel("интерпретация","истина",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"истиной","интерпретация связана с истиной","interpretation is related to truth","интерпретация",Some("интерпретация стремится к истине, но не гарантирует её"),None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Social — society, culture, tradition, order, progress
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("общество","человек",RelationType::RelRequires,ObjectCase::CaseGenitive,"человека","общество требует человека","society requires human","общество",None,None,None));
    graph.add_relation(rel("общество","закон",RelationType::RelStructures,ObjectCase::CaseAccusative,"закон","общество структурирует закон","society is structured by law","общество",None,None,None));
    graph.add_relation(rel("культура","язык",RelationType::RelRequires,ObjectCase::CaseGenitive,"языка","культура требует языка","culture requires language","культура",None,None,None));
    graph.add_relation(rel("культура","память",RelationType::RelPreserves,ObjectCase::CaseAccusative,"память","культура сохраняет память","culture preserves memory","культура",None,None,None));
    graph.add_relation(rel("традиция","культура",RelationType::RelIncludes,ObjectCase::CaseAccusative,"культуру","традиция входит в культуру","tradition is part of culture","традиция",None,None,None));
    graph.add_relation(rel("традиция","время",RelationType::RelDependsOn,ObjectCase::CaseAccusative,"время","традиция зависит от времени","tradition depends on time","традиция",None,None,None));
    graph.add_relation(rel("закон","справедливость",RelationType::RelDirectedAt,ObjectCase::CaseAccusative,"справедливость","закон направлен на справедливость","law is directed at justice","закон",None,None,None));
    graph.add_relation(rel("закон","свобода",RelationType::RelLimitedBy,ObjectCase::CaseInstrumental,"свободой","закон ограничивает свободу","law limits freedom","закон",Some("закон — это свобода, ограниченная ответственностью перед другими"),None,None));
    graph.add_relation(rel("порядок","хаос",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"хаосом","порядок контрастирует с хаосом","order contrasts with chaos","порядок",None,None,None));
    graph.add_relation(rel("порядок","закон",RelationType::RelRequires,ObjectCase::CaseGenitive,"закона","порядок требует закона","order requires law","порядок",None,None,None));
    graph.add_relation(rel("прогресс","время",RelationType::RelRequires,ObjectCase::CaseGenitive,"времени","прогресс требует времени","progress requires time","прогресс",None,None,None));
    graph.add_relation(rel("прогресс","традиция",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"традицией","прогресс контрастирует с традицией","progress contrasts with tradition","прогресс",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Aesthetics — harmony, art, creativity, form, beauty (extend)
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("гармония","красота",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"красотой","гармония связана с красотой","harmony is related to beauty","гармония",None,None,None));
    graph.add_relation(rel("гармония","порядок",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"порядком","гармония связана с порядком","harmony is related to order","гармония",None,None,None));
    graph.add_relation(rel("искусство","красота",RelationType::RelExpresses,ObjectCase::CaseAccusative,"красоту","искусство выражает красоту","art expresses beauty","искусство",None,None,None));
    graph.add_relation(rel("искусство","истина",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"истиной","искусство связано с истиной","art is related to truth","искусство",Some("искусство — это познание в образах"),None,None));
    graph.add_relation(rel("творчество","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","творчество требует свободы","creativity requires freedom","творчество",None,None,None));
    graph.add_relation(rel("творчество","воображение",RelationType::RelPresupposes,ObjectCase::CaseAccusative,"воображение","творчество предполагает воображение","creativity presupposes imagination","творчество",None,None,None));
    graph.add_relation(rel("прекрасное","истина",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"истиной","прекрасное связано с истиной","the beautiful is related to truth","прекрасное",None,None,None));
    graph.add_relation(rel("прекрасное","добро",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"добром","прекрасное связано с добром","the beautiful is related to the good","прекрасное",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Cross-domain bridges — connections between domains
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("истина","добро",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"добром","истина связана с добром","truth is related to good","истина",None,None,None));
    graph.add_relation(rel("знание","власть",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"властью","знание связано с властью","knowledge is related to power","знание",Some("знание даёт власть над обстоятельствами"),None,None));
    graph.add_relation(rel("свобода","творчество",RelationType::RelPresupposes,ObjectCase::CaseAccusative,"творчество","свобода предполагает творчество","freedom presupposes creativity","свобода",None,None,None));
    graph.add_relation(rel("сознание","значение",RelationType::RelDetermines,ObjectCase::CaseAccusative,"значение","сознание определяет значение","consciousness determines meaning","сознание",None,None,None));
    graph.add_relation(rel("язык","мышление",RelationType::RelStructures,ObjectCase::CaseAccusative,"мышление","язык структурирует мышление","language structures thinking","язык",None,None,None));
    graph.add_relation(rel("память","культура",RelationType::RelSupports,ObjectCase::CaseAccusative,"культуру","память поддерживает культуру","memory supports culture","память",None,None,None));
    graph.add_relation(rel("опыт","мудрость",RelationType::RelEvokes,ObjectCase::CaseAccusative,"мудрость","опыт порождает мудрость","experience evokes wisdom","опыт",Some("мудрость — это осмысленный опыт"),None,None));
    graph.add_relation(rel("мудрость","знание",RelationType::RelPresupposes,ObjectCase::CaseAccusative,"знание","мудрость предполагает знание","wisdom presupposes knowledge","мудрость",None,None,None));
    graph.add_relation(rel("мудрость","время",RelationType::RelRequires,ObjectCase::CaseGenitive,"времени","мудрость требует времени","wisdom requires time","мудрость",None,None,None));
    graph.add_relation(rel("время","смерть",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смертью","время связано со смертью","time is related to death","время",Some("время — мера конечности"),None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Bridge atoms — cross-domain connectors for spreading activation
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("человек","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","человек требует свободы","human requires freedom","человек",None,None,None));
    graph.add_relation(rel("человек","сознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"сознания","человек требует сознания","human requires consciousness","человек",None,None,None));
    graph.add_relation(rel("человек","общество",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"обществом","человек связан с обществом","human is related to society","человек",None,None,None));
    graph.add_relation(rel("человек","смерть",RelationType::RelLimitedBy,ObjectCase::CaseInstrumental,"смертью","человек ограничен смертью","human is limited by death","человек",Some("конечность — условие человеческого бытия"),None,None));
    graph.add_relation(rel("жизнь","время",RelationType::RelLimitedBy,ObjectCase::CaseInstrumental,"временем","жизнь ограничена временем","life is limited by time","жизнь",None,None,None));
    graph.add_relation(rel("жизнь","смысл",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смыслом","жизнь связана со смыслом","life is related to meaning","жизнь",Some("вопрос о смысле — вопрос о жизни"),None,None));
    graph.add_relation(rel("смысл","значение",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"значением","смысл связан со значением","sense is related to meaning","смысл",None,None,None));
    graph.add_relation(rel("смысл","понимание",RelationType::RelRequires,ObjectCase::CaseGenitive,"понимания","смысл требует понимания","sense requires understanding","смысл",None,None,None));
    graph.add_relation(rel("разум","истина",RelationType::RelDirectedAt,ObjectCase::CaseAccusative,"истину","разум направлен на истину","reason is directed at truth","разум",None,None,None));
    graph.add_relation(rel("чувство","разум",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"разумом","чувство контрастирует с разумом","feeling contrasts with reason","чувство",None,None,None));
    graph.add_relation(rel("чувство","красота",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"красотой","чувство связано с красотой","feeling is related to beauty","чувство",None,None,None));
    graph.add_relation(rel("счастье","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","счастье требует свободы","happiness requires freedom","счастье",None,None,None));
    graph.add_relation(rel("счастье","смысл",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смыслом","счастье связано со смыслом","happiness is related to sense","счастье",None,None,None));
    graph.add_relation(rel("страдание","смысл",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смыслом","страдание связано со смыслом","suffering is related to sense","страдание",Some("страдание ставит вопрос о смысле"),None,None));
    graph.add_relation(rel("страдание","смерть",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смертью","страдание связано со смертью","suffering is related to death","страдание",None,None,None));
    graph.add_relation(rel("природа","человек",RelationType::RelIncludes,ObjectCase::CaseAccusative,"человека","природа включает человека","nature includes human","природа",None,None,None));
    graph.add_relation(rel("природа","закон",RelationType::RelStructures,ObjectCase::CaseAccusative,"закон","природа структурирована законами","nature is structured by laws","природа",None,None,None));
    graph.add_relation(rel("вера","знание",RelationType::RelDiffersFrom,ObjectCase::CaseGenitive,"знания","вера отличается от знания","faith differs from knowledge","вера",Some("знание требует доказательств, вера — доверия"),None,None));
    graph.add_relation(rel("власть","знание",RelationType::RelRequires,ObjectCase::CaseGenitive,"знания","власть требует знания","power requires knowledge","власть",None,None,None));
    graph.add_relation(rel("игра","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","игра требует свободы","play requires freedom","игра",None,None,None));
    graph.add_relation(rel("игра","творчество",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"творчеством","игра связана с творчеством","play is related to creativity","игра",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Everyday concepts — practical, relatable topics
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("работа","смысл",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смыслом","работа связана со смыслом","work is related to meaning","работа",Some("работа без смысла — пустая трата жизни"),None,None));
    graph.add_relation(rel("работа","свобода",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"свободой","работа контрастирует со свободой","work contrasts with freedom","работа",Some("работа и свобода — вечный конфликт"),None,None));
    graph.add_relation(rel("работа","творчество",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"творчеством","работа связана с творчеством","work is related to creativity","работа",None,None,None));
    graph.add_relation(rel("деньги","свобода",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"свободой","деньги связаны со свободой","money is related to freedom","деньги",Some("деньги дают свободу, но не гарантируют счастье"),None,None));
    graph.add_relation(rel("деньги","счастье",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"счастьем","деньги связаны со счастьем","money is related to happiness","деньги",None,None,None));
    graph.add_relation(rel("деньги","власть",RelationType::RelSupports,ObjectCase::CaseAccusative,"власть","деньги поддерживают власть","money supports power","деньги",None,None,None));
    graph.add_relation(rel("здоровье","счастье",RelationType::RelRequires,ObjectCase::CaseGenitive,"счастья","здоровье необходимо для счастья","health is necessary for happiness","здоровье",None,None,None));
    graph.add_relation(rel("здоровье","время",RelationType::RelLimitedBy,ObjectCase::CaseInstrumental,"временем","здоровье ограничено временем","health is limited by time","здоровье",None,None,None));
    graph.add_relation(rel("здоровье","природа",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"природой","здоровье связано с природой","health is related to nature","здоровье",None,None,None));
    graph.add_relation(rel("дружба","доверие",RelationType::RelRequires,ObjectCase::CaseGenitive,"доверия","дружба требует доверия","friendship requires trust","дружба",Some("без доверия нет дружбы"),None,None));
    graph.add_relation(rel("дружба","любовь",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"любовью","дружба связана с любовью","friendship is related to love","дружба",None,None,None));
    graph.add_relation(rel("дружба","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","дружба требует свободы","friendship requires freedom","дружба",None,None,None));
    graph.add_relation(rel("семья","любовь",RelationType::RelRequires,ObjectCase::CaseGenitive,"любви","семья требует любви","family requires love","семья",None,None,None));
    graph.add_relation(rel("семья","ответственность",RelationType::RelRequires,ObjectCase::CaseGenitive,"ответственности","семья требует ответственности","family requires responsibility","семья",None,None,None));
    graph.add_relation(rel("семья","традиция",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"традицией","семья связана с традицией","family is related to tradition","семья",None,None,None));
    graph.add_relation(rel("образование","знание",RelationType::RelDirectedAt,ObjectCase::CaseAccusative,"знание","образование направлено на знание","education is directed at knowledge","образование",None,None,None));
    graph.add_relation(rel("образование","свобода",RelationType::RelSupports,ObjectCase::CaseAccusative,"свободу","образование поддерживает свободу","education supports freedom","образование",Some("знание освобождает"),None,None));
    graph.add_relation(rel("образование","мышление",RelationType::RelStructures,ObjectCase::CaseAccusative,"мышление","образование структурирует мышление","education structures thinking","образование",None,None,None));
    graph.add_relation(rel("музыка","красота",RelationType::RelExpresses,ObjectCase::CaseAccusative,"красоту","музыка выражает красоту","music expresses beauty","музыка",None,None,None));
    graph.add_relation(rel("музыка","эмоция",RelationType::RelEvokes,ObjectCase::CaseAccusative,"эмоции","музыка вызывает эмоции","music evokes emotion","музыка",None,None,None));
    graph.add_relation(rel("музыка","время",RelationType::RelStructures,ObjectCase::CaseAccusative,"время","музыка структурирует время","music structures time","музыка",None,None,None));
    graph.add_relation(rel("наука","истина",RelationType::RelDirectedAt,ObjectCase::CaseAccusative,"истину","наука направлена на истину","science is directed at truth","наука",None,None,None));
    graph.add_relation(rel("наука","знание",RelationType::RelExpresses,ObjectCase::CaseAccusative,"знание","наука выражает знание","science expresses knowledge","наука",None,None,None));
    graph.add_relation(rel("наука","доказательство",RelationType::RelRequires,ObjectCase::CaseGenitive,"доказательства","наука требует доказательства","science requires proof","наука",None,None,None));
    graph.add_relation(rel("технология","прогресс",RelationType::RelSupports,ObjectCase::CaseAccusative,"прогресс","технология поддерживает прогресс","technology supports progress","технология",None,None,None));
    graph.add_relation(rel("технология","человек",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"человеком","технология связана с человеком","technology is related to human","технология",Some("технология — продолжение человека"),None,None));
    graph.add_relation(rel("дом","покой",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"покоем","дом связан с покоем","home is related to peace","дом",None,None,None));
    graph.add_relation(rel("дом","семья",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"семьёй","дом связан с семьёй","home is related to family","дом",None,None,None));
    graph.add_relation(rel("путешествие","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","путешествие требует свободы","travel requires freedom","путешествие",None,None,None));
    graph.add_relation(rel("путешествие","познание",RelationType::RelEvokes,ObjectCase::CaseAccusative,"познание","путешествие пробуждает познание","travel evokes cognition","путешествие",None,None,None));
    graph.add_relation(rel("одиночество","свобода",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"свободой","одиночество связано со свободой","loneliness is related to freedom","одиночество",Some("одиночество — цена свободы"),None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Psychology — personality, motivation, stress, development
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("личность","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","личность требует свободы","personality requires freedom","личность",None,None,None));
    graph.add_relation(rel("личность","сознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"сознания","личность требует сознания","personality requires consciousness","личность",None,None,None));
    graph.add_relation(rel("личность","общество",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"обществом","личность связана с обществом","personality is related to society","личность",Some("личность формируется в обществе, но не сводится к нему"),None,None));
    graph.add_relation(rel("мотивация","желание",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"желанием","мотивация связана с желанием","motivation is related to desire","мотивация",None,None,None));
    graph.add_relation(rel("мотивация","смысл",RelationType::RelRequires,ObjectCase::CaseGenitive,"смысла","мотивация требует смысла","motivation requires meaning","мотивация",Some("без смысла мотивация угасает"),None,None));
    graph.add_relation(rel("мотивация","цель",RelationType::RelDirectedAt,ObjectCase::CaseAccusative,"цель","мотивация направлена на цель","motivation is directed at goal","мотивация",None,None,None));
    graph.add_relation(rel("стресс","свобода",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"свободой","стресс контрастирует со свободой","stress contrasts with freedom","стресс",Some("стресс — реакция на утрату контроля"),None,None));
    graph.add_relation(rel("стресс","здоровье",RelationType::RelDestroys,ObjectCase::CaseAccusative,"здоровье","стресс разрушает здоровье","stress destroys health","стресс",None,None,None));
    graph.add_relation(rel("стресс","время",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"временем","стресс связан со временем","stress is related to time","стресс",Some("стресс — это сжатие времени"),None,None));
    graph.add_relation(rel("развитие","время",RelationType::RelRequires,ObjectCase::CaseGenitive,"времени","развитие требует времени","development requires time","развитие",None,None,None));
    graph.add_relation(rel("развитие","опыт",RelationType::RelDependsOn,ObjectCase::CaseAccusative,"опыт","развитие опирается на опыт","development relies on experience","развитие",None,None,None));
    graph.add_relation(rel("развитие","личность",RelationType::RelDirectedAt,ObjectCase::CaseAccusative,"личность","развитие направлено на личность","development is directed at personality","развитие",None,None,None));
    graph.add_relation(rel("цель","смысл",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смыслом","цель связана со смыслом","goal is related to meaning","цель",None,None,None));
    graph.add_relation(rel("цель","действие",RelationType::RelRequires,ObjectCase::CaseGenitive,"действия","цель требует действия","goal requires action","цель",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Politics — state, democracy, equality, rights
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("государство","власть",RelationType::RelRequires,ObjectCase::CaseGenitive,"власти","государство требует власти","state requires power","государство",None,None,None));
    graph.add_relation(rel("государство","закон",RelationType::RelStructures,ObjectCase::CaseAccusative,"закон","государство структурировано законом","state is structured by law","государство",None,None,None));
    graph.add_relation(rel("государство","свобода",RelationType::RelLimitedBy,ObjectCase::CaseInstrumental,"свободой","государство ограничивает свободу","state limits freedom","государство",Some("парадокс: государство и защищает, и ограничивает свободу"),None,None));
    graph.add_relation(rel("демократия","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","демократия требует свободы","democracy requires freedom","демократия",None,None,None));
    graph.add_relation(rel("демократия","равенство",RelationType::RelRequires,ObjectCase::CaseGenitive,"равенства","демократия требует равенства","democracy requires equality","демократия",None,None,None));
    graph.add_relation(rel("демократия","мнение",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"мнением","демократия связана с мнением","democracy is related to opinion","демократия",Some("демократия — власть мнений"),None,None));
    graph.add_relation(rel("равенство","свобода",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"свободой","равенство контрастирует со свободой","equality contrasts with freedom","равенство",Some("абсолютное равенство убивает свободу, абсолютная свобода убивает равенство"),None,None));
    graph.add_relation(rel("равенство","справедливость",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"справедливостью","равенство связано со справедливостью","equality is related to justice","равенство",None,None,None));
    graph.add_relation(rel("права","свобода",RelationType::RelPresupposes,ObjectCase::CaseAccusative,"свободу","права предполагают свободу","rights presuppose freedom","права",None,None,None));
    graph.add_relation(rel("права","закон",RelationType::RelRequires,ObjectCase::CaseGenitive,"закона","права требуют закона","rights require law","права",None,None,None));
    graph.add_relation(rel("конфликт","интерес",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"интересом","конфликт связан с интересом","conflict is related to interest","конфликт",Some("конфликт — столкновение интересов"),None,None));
    graph.add_relation(rel("конфликт","понимание",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"пониманием","конфликт контрастирует с пониманием","conflict contrasts with understanding","конфликт",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Economics — resource, exchange, value, market, choice
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("ресурс","выбор",RelationType::RelRequires,ObjectCase::CaseGenitive,"выбора","ресурс требует выбора","resource requires choice","ресурс",Some("ограниченность ресурса — условие выбора"),None,None));
    graph.add_relation(rel("ресурс","труд",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"трудом","ресурс связан с трудом","resource is related to labor","ресурс",None,None,None));
    graph.add_relation(rel("ценность","смысл",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смыслом","ценность связана со смыслом","value is related to meaning","ценность",Some("мы ценим то, в чём видим смысл"),None,None));
    graph.add_relation(rel("ценность","обмен",RelationType::RelRequires,ObjectCase::CaseGenitive,"обмена","ценность требует обмена","value requires exchange","ценность",None,None,None));
    graph.add_relation(rel("обмен","доверие",RelationType::RelRequires,ObjectCase::CaseGenitive,"доверия","обмен требует доверия","exchange requires trust","обмен",None,None,None));
    graph.add_relation(rel("обмен","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","обмен требует свободы","exchange requires freedom","обмен",None,None,None));
    graph.add_relation(rel("выбор","ресурс",RelationType::RelLimitedBy,ObjectCase::CaseInstrumental,"ресурсом","выбор ограничен ресурсом","choice is limited by resource","выбор",None,None,None));
    graph.add_relation(rel("интерес","мотивация",RelationType::RelSupports,ObjectCase::CaseAccusative,"мотивацию","интерес поддерживает мотивацию","interest supports motivation","интерес",None,None,None));
    graph.add_relation(rel("интерес","знание",RelationType::RelEvokes,ObjectCase::CaseAccusative,"знание","интерес пробуждает знание","interest evokes knowledge","интерес",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Relationships — connection, attachment, respect, jealousy
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("отношения","доверие",RelationType::RelRequires,ObjectCase::CaseGenitive,"доверия","отношения требуют доверия","relationships require trust","отношения",Some("без доверия отношения превращаются в сделку"),None,None));
    graph.add_relation(rel("отношения","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","отношения требуют свободы","relationships require freedom","отношения",None,None,None));
    graph.add_relation(rel("отношения","понимание",RelationType::RelRequires,ObjectCase::CaseGenitive,"понимания","отношения требуют понимания","relationships require understanding","отношения",None,None,None));
    graph.add_relation(rel("уважение","личность",RelationType::RelPresupposes,ObjectCase::CaseAccusative,"личность","уважение предполагает личность","respect presupposes personality","уважение",None,None,None));
    graph.add_relation(rel("уважение","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","уважение требует свободы","respect requires freedom","уважение",Some("нельзя уважать того, кто не свободен"),None,None));
    graph.add_relation(rel("уважение","равенство",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"равенством","уважение связано с равенством","respect is related to equality","уважение",None,None,None));
    graph.add_relation(rel("ревность","доверие",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"доверием","ревность контрастирует с доверием","jealousy contrasts with trust","ревность",Some("ревность — это страх потери, доверие — уверенность"),None,None));
    graph.add_relation(rel("ревность","свобода",RelationType::RelDestroys,ObjectCase::CaseAccusative,"свободу","ревность разрушает свободу","jealousy destroys freedom","ревность",None,None,None));
    graph.add_relation(rel("привязанность","любовь",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"любовью","привязанность связана с любовью","attachment is related to love","привязанность",None,None,None));
    graph.add_relation(rel("привязанность","свобода",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"свободой","привязанность контрастирует со свободой","attachment contrasts with freedom","привязанность",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Career — success, talent, discipline, calling
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("успех","труд",RelationType::RelRequires,ObjectCase::CaseGenitive,"труда","успех требует труда","success requires labor","успех",Some("успех без труда — случайность, а не достижение"),None,None));
    graph.add_relation(rel("успех","время",RelationType::RelRequires,ObjectCase::CaseGenitive,"времени","успех требует времени","success requires time","успех",None,None,None));
    graph.add_relation(rel("успех","смысл",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смыслом","успех связан со смыслом","success is related to meaning","успех",None,None,None));
    graph.add_relation(rel("талант","труд",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"трудом","талант связан с трудом","talent is related to labor","талант",Some("талант без труда — потенциал, труд без таланта — ремесло"),None,None));
    graph.add_relation(rel("талант","природа",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"природой","талант связан с природой","talent is related to nature","талант",None,None,None));
    graph.add_relation(rel("дисциплина","свобода",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"свободой","дисциплина связана со свободой","discipline is related to freedom","дисциплина",Some("дисциплина — не враг свободы, а её инструмент"),None,None));
    graph.add_relation(rel("дисциплина","цель",RelationType::RelSupports,ObjectCase::CaseAccusative,"цель","дисциплина поддерживает цель","discipline supports goal","дисциплина",None,None,None));
    graph.add_relation(rel("призвание","смысл",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смыслом","призвание связано со смыслом","calling is related to meaning","призвание",None,None,None));
    graph.add_relation(rel("призвание","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","призвание требует свободы","calling requires freedom","призвание",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Modern life — internet, information, speed, choice overload
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("информация","знание",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"знанием","информация связана со знанием","information is related to knowledge","информация",Some("информация — сырьё, знание — продукт"),None,None));
    graph.add_relation(rel("информация","внимание",RelationType::RelRequires,ObjectCase::CaseGenitive,"внимания","информация требует внимания","information requires attention","информация",None,None,None));
    graph.add_relation(rel("внимание","сознание",RelationType::RelRequires,ObjectCase::CaseGenitive,"сознания","внимание требует сознания","attention requires consciousness","внимание",None,None,None));
    graph.add_relation(rel("внимание","время",RelationType::RelRequires,ObjectCase::CaseGenitive,"времени","внимание требует времени","attention requires time","внимание",Some("внимание — валюта современности"),None,None));
    graph.add_relation(rel("скорость","время",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"временем","скорость связана со временем","speed is related to time","скорость",None,None,None));
    graph.add_relation(rel("скорость","качество",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"качеством","скорость контрастирует с качеством","speed contrasts with quality","скорость",Some("быстро — не всегда хорошо"),None,None));
    graph.add_relation(rel("выбор","свобода",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"свободой","выбор связан со свободой","choice is related to freedom","выбор",None,None,None));
    graph.add_relation(rel("выбор","информация",RelationType::RelRequires,ObjectCase::CaseGenitive,"информации","выбор требует информации","choice requires information","выбор",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Emotions & well-being — joy, sadness, anger, calm, sleep, food
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("радость","смысл",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смыслом","радость связана со смыслом","joy is related to meaning","радость",Some("радость — это переживание смысла"),None,None));
    graph.add_relation(rel("радость","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","радость требует свободы","joy requires freedom","радость",None,None,None));
    graph.add_relation(rel("грусть","смысл",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"смыслом","грусть связана со смыслом","sadness is related to meaning","грусть",Some("грусть — это реакция на утрату смысла"),None,None));
    graph.add_relation(rel("грусть","время",RelationType::RelRequires,ObjectCase::CaseGenitive,"времени","грусть требует времени","sadness requires time","грусть",None,None,None));
    graph.add_relation(rel("гнев","свобода",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"свободой","гнев контрастирует со свободой","anger contrasts with freedom","гнев",Some("гнев — реакция на ограничение свободы"),None,None));
    graph.add_relation(rel("гнев","справедливость",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"справедливостью","гнев связан со справедливостью","anger is related to justice","гнев",None,None,None));
    graph.add_relation(rel("спокойствие","свобода",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"свободой","спокойствие связано со свободой","calm is related to freedom","спокойствие",Some("спокойствие — свобода от внутреннего шума"),None,None));
    graph.add_relation(rel("спокойствие","время",RelationType::RelRequires,ObjectCase::CaseGenitive,"времени","спокойствие требует времени","calm requires time","спокойствие",None,None,None));
    graph.add_relation(rel("сон","здоровье",RelationType::RelRequires,ObjectCase::CaseGenitive,"здоровья","сон требует здоровья","sleep requires health","сон",None,None,None));
    graph.add_relation(rel("сон","время",RelationType::RelStructures,ObjectCase::CaseAccusative,"время","сон структурирует время","sleep structures time","сон",None,None,None));
    graph.add_relation(rel("еда","жизнь",RelationType::RelRequires,ObjectCase::CaseGenitive,"жизни","еда необходима для жизни","food is necessary for life","еда",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Ethics & society extended — morality, justice system, ethics
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("мораль","добро",RelationType::RelDirectedAt,ObjectCase::CaseAccusative,"добро","мораль направлена на добро","morality is directed at good","мораль",None,None,None));
    graph.add_relation(rel("мораль","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","мораль требует свободы","morality requires freedom","мораль",Some("мораль без свободы — дрессура"),None,None));
    graph.add_relation(rel("этика","мораль",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"моралью","этика связана с моралью","ethics is related to morality","этика",None,None,None));
    graph.add_relation(rel("этика","разум",RelationType::RelRequires,ObjectCase::CaseGenitive,"разума","этика требует разума","ethics requires reason","этика",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Systems & methods — structure, process, method, system
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("система","порядок",RelationType::RelRequires,ObjectCase::CaseGenitive,"порядка","система требует порядка","system requires order","система",None,None,None));
    graph.add_relation(rel("система","свобода",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"свободой","система контрастирует со свободой","system contrasts with freedom","система",Some("система даёт структуру, но ограничивает спонтанность"),None,None));
    graph.add_relation(rel("метод","цель",RelationType::RelSupports,ObjectCase::CaseAccusative,"цель","метод поддерживает цель","method supports goal","метод",None,None,None));
    graph.add_relation(rel("метод","знание",RelationType::RelReliesOn,ObjectCase::CaseAccusative,"знание","метод опирается на знание","method relies on knowledge","метод",None,None,None));
    graph.add_relation(rel("процесс","время",RelationType::RelRequires,ObjectCase::CaseGenitive,"времени","процесс требует времени","process requires time","процесс",None,None,None));
    graph.add_relation(rel("процесс","результат",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"результатом","процесс связан с результатом","process is related to result","процесс",Some("важен и путь, и итог"),None,None));
    graph.add_relation(rel("результат","труд",RelationType::RelRequires,ObjectCase::CaseGenitive,"труда","результат требует труда","result requires labor","результат",None,None,None));

    // ═══════════════════════════════════════════════════════════════════
    // Communication — dialogue, listening, argument
    // ═══════════════════════════════════════════════════════════════════
    graph.add_relation(rel("диалог","понимание",RelationType::RelDirectedAt,ObjectCase::CaseAccusative,"понимание","диалог направлен на понимание","dialogue is directed at understanding","диалог",None,None,None));
    graph.add_relation(rel("диалог","свобода",RelationType::RelRequires,ObjectCase::CaseGenitive,"свободы","диалог требует свободы","dialogue requires freedom","диалог",None,None,None));
    graph.add_relation(rel("диалог","уважение",RelationType::RelRequires,ObjectCase::CaseGenitive,"уважения","диалог требует уважения","dialogue requires respect","диалог",None,None,None));
    graph.add_relation(rel("спор","истина",RelationType::RelRelatedTo,ObjectCase::CaseInstrumental,"истиной","спор связан с истиной","argument is related to truth","спор",Some("в споре рождается истина"),None,None));
    graph.add_relation(rel("спор","уважение",RelationType::RelContrastsWith,ObjectCase::CaseInstrumental,"уважением","спор контрастирует с уважением","argument contrasts with respect","спор",None,None,None));
    graph
}

/// Verbalize a relation into Russian surface text.
/// Uses `ru_original` (hand-written grammatically correct sentence) when available.
/// Falls back to morphological assembly from parts for runtime-generated relations.
pub fn verbalize_relation(rel: &Relation) -> String {
    if !rel.ru_original.trim().is_empty() {
        return rel.ru_original.clone();
    }
    if let Some(verb) = &rel.verb_override {
        format!("{} {} {}", rel.from.as_str(), verb, rel.object_text)
    } else {
        format!(
            "{} {} {}",
            rel.from.as_str(),
            rel.rel_type.verb_ru(),
            rel.object_text
        )
    }
}

/// Verbalize a path proof into text.
pub fn verbalize_path(proof: &PathProof) -> String {
    proof
        .edges
        .iter()
        .map(verbalize_relation)
        .collect::<Vec<_>>()
        .join(". ")
}
