#!/usr/bin/env python3
"""Generate the claim-addressed ResponsePlan V2 audited corpus manifest."""
import hashlib
import json
import struct
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GATE_DIR = ROOT / "data" / "gates" / "response-plan-v2"
TSV = ROOT / "qxfx0-semantic" / "assets" / "argued_topics.tsv"
VALENCY = ROOT / "qxfx0-plan-v2" / "assets" / "valency_frames.tsv"
VERB_LEXICON = ROOT / "data" / "verb_lexemes.json"
ADJECTIVE_LEXICON = ROOT / "data" / "adjective_lexemes.json"
PACK = ROOT / "data" / "packs" / "philosophy-core-v1"
OUT = GATE_DIR / "audited-corpus-manifest.json"

SCHEMA_VERSION = 2
MANIFEST_ID = "response-plan-v2-audited-corpus-v2"
GOVERNED_ONLY_TOPICS = {
    "мнение", "вера", "доверие", "справедливость", "разум", "бытие",
    "история", "воля", "смерть", "одиночество", "любовь", "труд",
    "покой", "власть", "молчание", "страх", "время", "язык",
}

def absorb(value: str) -> bytes:
    data = value.encode("utf-8")
    return struct.pack(">Q", len(data)) + data


def proposition_id(record: dict) -> str:
    payload = [record["subject"], record["relation"], record["object"]]
    body = b"qxfx0:proposition:v1" + absorb("predicate") + struct.pack(">Q", 3)
    body += b"".join(absorb(value) for value in payload) + struct.pack(">Q", 0)
    return hashlib.sha256(body).hexdigest()


def discourse_digest(items: list[tuple[str, str]]) -> str:
    body = b"qxfx0:discourse:v1" + absorb("sequence") + struct.pack(">Q", len(items))
    for role, proposition in items:
        body += absorb(role) + absorb(proposition)
    return hashlib.sha256(body).hexdigest()


def claim_id(proposition: str, path: str) -> str:
    return hashlib.sha256(
        b"qxfx0:claim:v1" + absorb(proposition) + absorb(path)
    ).hexdigest()


def sha256_bytes(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def valency_fingerprint(path: Path, frames: list[dict]) -> str:
    """Mirror of `qxfx0_plan_v2::valency` fingerprinting (v2 domain): the
    TSV bytes plus the digest over every frame's derived paradigm."""
    digest = frames_conjugation_digest(frames)
    hasher = hashlib.sha256()
    hasher.update(b"qxfx0:valency-lexicon:v2")
    hasher.update(path.read_bytes())
    hasher.update(digest.encode())
    return hasher.hexdigest()


def absorb_into(hasher, value: str) -> None:
    data = value.encode("utf-8")
    hasher.update(struct.pack(">Q", len(data)))
    hasher.update(data)


FINITE_KEYS = ["f1sg", "f2sg", "f3sg", "f1pl", "f2pl", "f3pl", "pm", "pf", "pn", "ppl"]
SHORT_KEYS = ["short_m", "short_f", "short_n", "short_pl"]


def frame_paradigm_digest(frame: dict, verb_forms, adjective_forms) -> str:
    hasher = hashlib.sha256()
    hasher.update(b"qxfx0:valency-conjugation:v1")
    absorb_into(hasher, frame["relation_id"])
    absorb_into(hasher, frame["conjugation"])
    # Only paradigm strategies carry a lemma in the Rust ConjugationStrategy;
    # a pinned row's head_lemma is documentation and stays out of the digest.
    if frame["conjugation"] in ("finite3", "agreeing_short") and frame["head_lemma"]:
        absorb_into(hasher, frame["head_lemma"])
    if frame["conjugation"] == "finite3":
        entry = verb_forms.get(frame["head_lemma"])
        if entry is not None:
            for key in FINITE_KEYS:
                absorb_into(hasher, key)
                absorb_into(hasher, entry.get(key, ""))
    elif frame["conjugation"] == "agreeing_short":
        entry = adjective_forms.get(frame["head_lemma"])
        if entry is not None:
            for key in SHORT_KEYS:
                absorb_into(hasher, key)
                absorb_into(hasher, entry.get(key, ""))
    return hasher.hexdigest()


def frames_conjugation_digest(frames: list[dict]) -> str:
    hasher = hashlib.sha256()
    hasher.update(b"qxfx0:valency-conjugations:v1")
    hasher.update(struct.pack(">Q", len(frames)))
    for frame in sorted(frames, key=lambda item: item["relation_id"]):
        absorb_into(hasher, frame["relation_id"])
        absorb_into(hasher, frame["paradigm_digest"])
    return hasher.hexdigest()


def valency_frames(path: Path, verb_forms, adjective_forms) -> list[dict]:
    """Parsed + paradigm-verified frames, mirroring the Rust loader.

    Verification included: a finite3 surface that disagrees with the verb
    lexicon's f3sg fails the tool, exactly like the embedded loader fails
    the release build.
    """
    frames = []
    for line in path.read_text().splitlines():
        if not line.strip() or line.startswith("#") or line.startswith("relation_id\t"):
            continue
        relation_id, head_kind, forms, _complement, head_lemma, strategy = line.split("\t")
        frame = {
            "relation_id": relation_id,
            "head_kind": head_kind,
            "head_forms": forms.split(",") if head_kind == "agreeing" else [forms],
            "head_lemma": None if head_lemma == "—" else head_lemma,
            "conjugation": strategy,
        }
        if strategy == "finite3":
            derived = verb_forms.get(head_lemma, {}).get("f3sg")
            if derived != forms:
                raise SystemExit(
                    f"valency row {relation_id}: pinned '{forms}' but "
                    f"{head_lemma} conjugates to '{derived}'"
                )
        if strategy == "agreeing_short":
            for key, pinned in zip(SHORT_KEYS, frame["head_forms"]):
                derived = adjective_forms.get(head_lemma, {}).get(key)
                if derived != pinned:
                    raise SystemExit(
                        f"valency row {relation_id}: pinned '{pinned}' but "
                        f"{head_lemma} carries '{derived}' in {key}"
                    )
        frames.append(frame)
    for frame in frames:
        frame["paradigm_digest"] = frame_paradigm_digest(frame, verb_forms, adjective_forms)
    return frames


def lexicon_forms(path: Path) -> dict:
    payload = json.loads(path.read_text())
    return {entry["lemma"]: entry["forms"] for entry in payload["lemmas"]}


def whole_word(surface: str, candidate: str) -> bool:
    return re.search(
        rf"(?<!\w){re.escape(candidate)}(?!\w)", surface, re.IGNORECASE
    ) is not None


def lexical_witnesses(surface: str, subject_semantic_id: str,
                       subject_binding: str, relation_semantic_id: str,
                       relation_binding: str | None,
                       heads: dict[str, list[str]]) -> list[dict]:
    witnesses = []
    subject = subject_semantic_id.removeprefix("concept.")
    if whole_word(surface, subject):
        witnesses.append({
            "kind": "subject_lemma",
            "source_semantic_id": subject_semantic_id,
            "source_binding": subject_binding,
            "accepted_surfaces": [subject],
        })
    head_surfaces = heads.get(relation_binding, []) if relation_binding else []
    if any(whole_word(surface, head) for head in head_surfaces):
        witnesses.append({
            "kind": "head",
            "source_semantic_id": relation_semantic_id,
            "source_binding": relation_binding,
            "accepted_surfaces": head_surfaces,
        })
    return witnesses


facts = {
    item["record"]["id"]: item["record"]
    for item in json.loads((PACK / "facts.json").read_text())
}
verb_forms = lexicon_forms(VERB_LEXICON)
adjective_forms = lexicon_forms(ADJECTIVE_LEXICON)
frames = valency_frames(VALENCY, verb_forms, adjective_forms)
heads = {frame["relation_id"]: frame["head_forms"] for frame in frames}
lines = [
    line for line in TSV.read_text().splitlines()
    if line.strip() and not line.startswith("#")
][1:]
topics = {}
claims_total = 0
for line in lines:
    cells = line.split("\t")
    topic, predicate = cells[0], cells[1]
    surfaces = [cells[5], cells[6]] + ([cells[7]] if len(cells) > 7 and cells[7] else [])
    fact_ids = [f"fact.{predicate}", f"fact.{predicate}.counterpoint"]
    roles = ["thesis", "counterpoint"]
    if len(surfaces) == 3:
        fact_ids.append(f"fact.{predicate}.consequence")
        roles.append("consequence")
    propositions = [proposition_id(facts[fact]) for fact in fact_ids]
    root = discourse_digest(list(zip(roles, propositions)))
    claims = {}
    for index, (role, proposition, fact, surface) in enumerate(
        zip(roles, propositions, fact_ids, surfaces)
    ):
        path = f"{index}.{role}"
        row = {
            "discourse_root_digest": root,
            "canonical_path": path,
            "fact_id": fact,
            "proposition_id": proposition,
            "approved_surface": surface,
            "approved_surface_sha256": hashlib.sha256(surface.encode()).hexdigest(),
            "realization_strategy": "clause" if role == "thesis" else "fixed_phrase",
            "surface_validation": (
                "exact_clause" if role == "thesis" and topic not in GOVERNED_ONLY_TOPICS
                else "governed_clause" if role == "thesis" else "audited_verbatim"
            ),
        }
        current = facts[fact]
        primary = facts[fact_ids[0]]
        row["lexical_witnesses"] = lexical_witnesses(
            surface,
            current["subject"],
            cells[2],
            current["relation"],
            cells[3] if current["relation"] == primary["relation"] else None,
            heads,
        )
        if role == "thesis" and topic not in GOVERNED_ONLY_TOPICS:
            row["expected_clause_surface_sha256"] = row["approved_surface_sha256"]
        claims[claim_id(proposition, path)] = row
        claims_total += 1
    topics[topic] = {"claims": claims}

manifest = {
    "schema_version": SCHEMA_VERSION,
    "manifest_id": MANIFEST_ID,
    "source_files": {
        "argued_topics.tsv": sha256_bytes(TSV),
        "valency_frames.tsv": valency_fingerprint(VALENCY, frames),
        "manifest.json": sha256_bytes(PACK / "manifest.json"),
        "concepts.json": sha256_bytes(PACK / "concepts.json"),
        "facts.json": sha256_bytes(PACK / "facts.json"),
        "relations.json": sha256_bytes(PACK / "relations.json"),
    },
    "diagnostics": {
        "topics_total": len(topics),
        "claims_total": claims_total,
        "exact_clause_surfaces": len(topics) - len(GOVERNED_ONLY_TOPICS),
        "governed_clause_surfaces": len(GOVERNED_ONLY_TOPICS),
        "fixed_phrase_surfaces": claims_total - len(topics),
    },
    "topics": topics,
}
canonical = json.dumps(
    manifest, ensure_ascii=False, sort_keys=True, separators=(",", ":")
).encode()
manifest["manifest_digest"] = hashlib.sha256(
    b"qxfx0:audited-corpus-manifest:v2" + struct.pack(">Q", len(canonical)) + canonical
).hexdigest()
OUT.write_text(json.dumps(manifest, ensure_ascii=False, sort_keys=True, indent=2) + "\n")
print(f"wrote {OUT}")
print(f"topics={len(topics)} claims={claims_total} digest={manifest['manifest_digest'][:16]}")
