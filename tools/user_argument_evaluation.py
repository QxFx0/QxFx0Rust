#!/usr/bin/env python3
"""Validate reviewed user-argument gold data and score typed predictions."""

import argparse
import collections
import copy
import hashlib
import json
from pathlib import Path
import re
import unicodedata


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "data/gates/user-argument/gold-corpus-v1.json"

MANIFEST_SCHEMA = "qxfx0.user-argument-gold-corpus.v1"
COMPILED_SCHEMA = "qxfx0.user-argument-gold-compiled.v1"
PREDICTIONS_SCHEMA = "qxfx0.user-argument-predictions.v1"
REPORT_SCHEMA = "qxfx0.user-argument-evaluation-report.v1"
MANIFEST_DOMAIN = b"qxfx0.user-argument-gold-corpus.v1\0"
REPORT_DOMAIN = b"qxfx0.user-argument-evaluation-report.v1\0"

ID_PATTERN = re.compile(r"^[a-z0-9][a-z0-9._-]{0,255}$")
SHA256_PATTERN = re.compile(r"^[0-9a-f]{64}$")
BUILD_SHA_PATTERN = re.compile(r"^[0-9a-f]{40}$")

REQUIRED_CATEGORIES = {
    "clean_argument",
    "enthymeme",
    "unsupported_assertion",
    "counterexample",
    "concession",
    "revision",
    "contradiction",
    "evidence_request",
    "definition_request",
    "quotation",
    "hypothetical",
    "negation",
    "sarcasm_probe",
    "malformed_input",
    "external_subject",
    "unknown_topic",
    "undercut",
}
REQUIRED_FORMULATION_CLASSES = {"direct", "quoted", "hypothetical", "negated", "ambiguous"}
NODE_KINDS = {"claim", "premise", "conclusion", "qualifier", "counterclaim"}
SOURCE_CLASSES = {"direct", "quoted", "reported", "hypothetical", "unknown"}
POLARITIES = {"affirmed", "negated", "unknown"}
PREDICATES = {
    "is",
    "defines",
    "has_property",
    "causes",
    "enables",
    "prevents",
    "requires",
    "permits",
    "prohibits",
    "values",
    "justifies",
    "follows_from",
    "contradicts",
    "needs_evidence",
    "needs_definition",
}
SUBJECT_KINDS = {"canonical_topic", "unresolved_topic", "external_subject", "dialogue", "no_topic"}
OBJECT_KINDS = {
    "canonical_topic",
    "fact",
    "unresolved_topic",
    "external_subject",
    "evidence",
    "definition",
}
RELATION_KINDS = {
    "supports",
    "attacks",
    "qualifies",
    "rebuts",
    "undercuts",
    "entails",
    "contradicts",
    "requests_evidence",
    "requests_definition",
}
DISPOSITIONS = {"parsed", "partial", "abstained"}
OMISSION_REASONS = {
    "ambiguous_attachment",
    "unresolved_proposition",
    "quoted_position_ambiguity",
    "unsupported_relation",
    "negation_ambiguity",
    "insufficient_evidence",
}
FORBIDDEN_ARTIFACT_KEYS = {
    "raw_input",
    "raw_span",
    "raw_text",
    "utterance",
    "formulation",
    "response",
    "response_text",
    "session_id",
    "request_id",
    "user_id",
    "user_label",
    "character_offset",
    "span_start",
    "span_end",
}


class ValidationError(ValueError):
    pass


def load_json(path):
    """Load one UTF-8 JSON document and normalize decode failures."""
    try:
        return json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValidationError(f"invalid JSON in {path}: {error}") from error


def reject_unknown_fields(label, value, allowed):
    """Reject non-object values and fields outside a closed schema."""
    if not isinstance(value, dict):
        raise ValidationError(f"{label} must be an object")
    unknown = sorted(set(value) - set(allowed))
    if unknown:
        raise ValidationError(f"{label} contains unknown fields: {unknown}")


def validate_id(field, value):
    """Validate a bounded ASCII metadata or graph identifier."""
    if not isinstance(value, str) or not ID_PATTERN.fullmatch(value):
        raise ValidationError(f"{field} is not a bounded stable identifier")


def validate_contract_id(field, value):
    """Validate a visible bounded identifier used by the Rust contract."""
    if not isinstance(value, str) or not value or len(value.encode("utf-8")) > 256:
        raise ValidationError(f"{field} is not a bounded contract identifier")
    if any(character.isspace() or unicodedata.category(character) in {"Cc", "Cf"} for character in value):
        raise ValidationError(f"{field} contains whitespace, control, or format characters")


def validate_sha256(field, value):
    """Validate one lowercase SHA-256 hexadecimal digest."""
    if not isinstance(value, str) or not SHA256_PATTERN.fullmatch(value):
        raise ValidationError(f"{field} must be a lowercase SHA-256 digest")


def canonical_digest(value, domain):
    """Hash canonical JSON with domain separation and length framing."""
    encoded = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode(
        "utf-8"
    )
    digest = hashlib.sha256(domain)
    digest.update(len(encoded).to_bytes(8, "big"))
    digest.update(encoded)
    return digest.hexdigest()


def manifest_digest(manifest):
    """Calculate the gold manifest digest without its recorded digest field."""
    payload = copy.deepcopy(manifest)
    payload.pop("manifest_digest", None)
    return canonical_digest(payload, MANIFEST_DOMAIN)


def validate_term(label, term, allowed_kinds):
    """Validate a closed proposition subject or object term."""
    reject_unknown_fields(label, term, {"kind", "id"})
    kind = term.get("kind")
    if kind not in allowed_kinds:
        raise ValidationError(f"{label} has unsupported kind {kind!r}")
    identity_kinds = {"canonical_topic", "fact"}
    if kind in identity_kinds:
        validate_contract_id(f"{label}.id", term.get("id"))
    elif "id" in term:
        raise ValidationError(f"{label} categorical kind cannot carry an identifier")


def validate_proposition(label, proposition):
    """Validate one text-free normalized proposition."""
    reject_unknown_fields(label, proposition, {"subject", "predicate", "object"})
    validate_term(f"{label}.subject", proposition.get("subject"), SUBJECT_KINDS)
    if proposition.get("predicate") not in PREDICATES:
        raise ValidationError(f"{label} has unsupported predicate")
    if "object" in proposition:
        validate_term(f"{label}.object", proposition["object"], OBJECT_KINDS)


def validate_expected_node(case_id, node):
    """Validate one reviewed expected node."""
    reject_unknown_fields(
        f"{case_id}.expected_node",
        node,
        {
            "node_id",
            "kind",
            "source",
            "polarity",
            "proposition",
            "confidence_min_basis_points",
        },
    )
    validate_id("expected_node.node_id", node.get("node_id"))
    if node.get("kind") not in NODE_KINDS:
        raise ValidationError(f"{case_id}: unsupported node kind")
    if node.get("source") not in SOURCE_CLASSES:
        raise ValidationError(f"{case_id}: unsupported source class")
    if node.get("polarity") not in POLARITIES:
        raise ValidationError(f"{case_id}: unsupported polarity")
    confidence = node.get("confidence_min_basis_points")
    if not isinstance(confidence, int) or isinstance(confidence, bool) or not 0 <= confidence <= 10_000:
        raise ValidationError(f"{case_id}: invalid node confidence floor")
    validate_proposition(f"{case_id}.expected_node.proposition", node.get("proposition"))


def validate_expected_relation(case_id, relation, node_ids):
    """Validate one reviewed expected relation and its references."""
    reject_unknown_fields(
        f"{case_id}.expected_relation",
        relation,
        {"from", "to", "kind", "confidence_min_basis_points"},
    )
    source = relation.get("from")
    target = relation.get("to")
    validate_id("expected_relation.from", source)
    validate_id("expected_relation.to", target)
    if source not in node_ids or target not in node_ids:
        raise ValidationError(f"{case_id}: expected relation has a dangling node")
    if source == target:
        raise ValidationError(f"{case_id}: expected relation is a self-relation")
    if relation.get("kind") not in RELATION_KINDS:
        raise ValidationError(f"{case_id}: unsupported relation kind")
    confidence = relation.get("confidence_min_basis_points")
    if not isinstance(confidence, int) or isinstance(confidence, bool) or not 0 <= confidence <= 10_000:
        raise ValidationError(f"{case_id}: invalid relation confidence floor")


def validate_case(case):
    """Validate one reviewed gold case and return its coverage inventory."""
    reject_unknown_fields(
        "gold case",
        case,
        {
            "case_id",
            "category",
            "formulation",
            "formulation_sha256",
            "review_status",
            "formulation_classes",
            "privacy_needles",
            "accepted_dispositions",
            "expected_nodes",
            "expected_relations",
            "expected_omissions",
        },
    )
    case_id = case.get("case_id")
    validate_id("case_id", case_id)
    if case.get("category") not in REQUIRED_CATEGORIES:
        raise ValidationError(f"{case_id}: unsupported category")
    formulation = case.get("formulation")
    if not isinstance(formulation, str) or not formulation.strip() or len(formulation) > 512:
        raise ValidationError(f"{case_id}: formulation must contain 1-512 characters")
    if any(unicodedata.category(character) == "Cc" for character in formulation):
        raise ValidationError(f"{case_id}: formulation contains a control character")
    validate_sha256("formulation_sha256", case.get("formulation_sha256"))
    if hashlib.sha256(formulation.encode("utf-8")).hexdigest() != case["formulation_sha256"]:
        raise ValidationError(f"{case_id}: formulation digest mismatch")
    if case.get("review_status") != "approved":
        raise ValidationError(f"{case_id}: formulation is not approved")

    formulation_classes = case.get("formulation_classes")
    if (
        not isinstance(formulation_classes, list)
        or not formulation_classes
        or any(value not in REQUIRED_FORMULATION_CLASSES for value in formulation_classes)
        or len(formulation_classes) != len(set(formulation_classes))
    ):
        raise ValidationError(f"{case_id}: invalid formulation classes")
    needles = case.get("privacy_needles")
    if not isinstance(needles, list) or any(
        not isinstance(value, str)
        or not value
        or len(value) > 256
        or any(unicodedata.category(character) in {"Cc", "Cf"} for character in value)
        for value in needles
    ):
        raise ValidationError(f"{case_id}: invalid privacy needles")

    dispositions = case.get("accepted_dispositions")
    if (
        not isinstance(dispositions, list)
        or not dispositions
        or any(value not in DISPOSITIONS for value in dispositions)
        or len(dispositions) != len(set(dispositions))
    ):
        raise ValidationError(f"{case_id}: invalid accepted dispositions")
    nodes = case.get("expected_nodes")
    relations = case.get("expected_relations")
    omissions = case.get("expected_omissions")
    if not isinstance(nodes, list) or len(nodes) > 16:
        raise ValidationError(f"{case_id}: expected_nodes must be a bounded list")
    if not isinstance(relations, list) or len(relations) > 32:
        raise ValidationError(f"{case_id}: expected_relations must be a bounded list")
    if (
        not isinstance(omissions, list)
        or len(omissions) > 16
        or any(value not in OMISSION_REASONS for value in omissions)
    ):
        raise ValidationError(f"{case_id}: invalid expected omissions")
    if len(omissions) != len(set(omissions)):
        raise ValidationError(f"{case_id}: duplicate expected omission")

    node_ids = set()
    for node in nodes:
        validate_expected_node(case_id, node)
        if node["node_id"] in node_ids:
            raise ValidationError(f"{case_id}: duplicate expected node")
        node_ids.add(node["node_id"])
    relation_tuples = set()
    for relation in relations:
        validate_expected_relation(case_id, relation, node_ids)
        relation_tuple = (relation["from"], relation["to"], relation["kind"])
        if relation_tuple in relation_tuples:
            raise ValidationError(f"{case_id}: duplicate expected relation")
        relation_tuples.add(relation_tuple)

    encoded_expectations = json.dumps(
        [nodes, relations, omissions], ensure_ascii=False, sort_keys=True
    ).casefold()
    if any(needle.casefold() in encoded_expectations for needle in needles):
        raise ValidationError(f"{case_id}: privacy needle appears in the expected graph")

    if not nodes and "abstained" not in dispositions:
        raise ValidationError(f"{case_id}: empty graph must accept abstention")
    if "parsed" in dispositions and omissions:
        raise ValidationError(f"{case_id}: parsed disposition cannot require omissions")
    if "partial" in dispositions and not omissions:
        raise ValidationError(f"{case_id}: partial disposition requires an omission")
    if dispositions == ["abstained"] and (nodes or relations or not omissions):
        raise ValidationError(f"{case_id}: abstained case has invalid expectations")
    return {
        "case_id": case_id,
        "category": case["category"],
        "classes": formulation_classes,
        "relations": [relation["kind"] for relation in relations],
        "sources": [node["source"] for node in nodes],
        "polarities": [node["polarity"] for node in nodes],
        "dispositions": dispositions,
    }


def validate_manifest(manifest, *, verify_digest=True):
    """Validate the complete corpus, coverage axes, policies, and digest."""
    reject_unknown_fields(
        "manifest",
        manifest,
        {
            "schema",
            "corpus_id",
            "authority_change",
            "persistence_change",
            "raw_user_logs",
            "reviewed_formulations_only",
            "source_policy",
            "required_categories",
            "required_relation_kinds",
            "required_formulation_classes",
            "cases",
            "manifest_digest",
        },
    )
    if manifest.get("schema") != MANIFEST_SCHEMA:
        raise ValidationError("unsupported gold corpus schema")
    validate_id("corpus_id", manifest.get("corpus_id"))
    if manifest.get("authority_change") != "none" or manifest.get("persistence_change") != "none":
        raise ValidationError("gold corpus cannot change authority or persistence")
    if manifest.get("raw_user_logs") is not False:
        raise ValidationError("raw_user_logs must be false")
    if manifest.get("reviewed_formulations_only") is not True:
        raise ValidationError("reviewed_formulations_only must be true")
    if manifest.get("source_policy") != "curated_synthetic_only":
        raise ValidationError("source_policy must prohibit production logs")
    declarations = (
        ("required_categories", REQUIRED_CATEGORIES),
        ("required_relation_kinds", RELATION_KINDS),
        ("required_formulation_classes", REQUIRED_FORMULATION_CLASSES),
    )
    for field, expected in declarations:
        values = manifest.get(field)
        if not isinstance(values, list) or set(values) != expected or len(values) != len(set(values)):
            raise ValidationError(f"{field} does not match the v1 contract")

    cases = manifest.get("cases")
    if not isinstance(cases, list) or not 17 <= len(cases) <= 64:
        raise ValidationError("gold corpus must contain 17-64 cases")
    inventories = [validate_case(case) for case in cases]
    case_ids = [item["case_id"] for item in inventories]
    if len(case_ids) != len(set(case_ids)):
        raise ValidationError("gold corpus contains duplicate case IDs")
    covered_categories = {item["category"] for item in inventories}
    covered_classes = {value for item in inventories for value in item["classes"]}
    covered_relations = {value for item in inventories for value in item["relations"]}
    covered_sources = {value for item in inventories for value in item["sources"]}
    covered_polarities = {value for item in inventories for value in item["polarities"]}
    covered_dispositions = {value for item in inventories for value in item["dispositions"]}
    if covered_categories != REQUIRED_CATEGORIES:
        raise ValidationError(f"gold corpus category coverage mismatch: {sorted(covered_categories)}")
    if covered_classes != REQUIRED_FORMULATION_CLASSES:
        raise ValidationError(f"gold corpus formulation-class coverage mismatch: {sorted(covered_classes)}")
    if covered_relations != RELATION_KINDS:
        raise ValidationError(f"gold corpus relation coverage mismatch: {sorted(covered_relations)}")
    for label, covered, expected in (
        ("source class", covered_sources, SOURCE_CLASSES),
        ("polarity", covered_polarities, POLARITIES),
        ("accepted disposition", covered_dispositions, DISPOSITIONS),
    ):
        if covered != expected:
            raise ValidationError(f"gold corpus {label} coverage mismatch: {sorted(covered)}")
    validate_sha256("manifest_digest", manifest.get("manifest_digest"))
    actual_digest = manifest_digest(manifest)
    if verify_digest and manifest["manifest_digest"] != actual_digest:
        raise ValidationError(
            f"manifest digest mismatch: recorded={manifest['manifest_digest']}, actual={actual_digest}"
        )
    return {
        "cases_total": len(cases),
        "manifest_digest": actual_digest,
        "category_distribution": dict(sorted(collections.Counter(item["category"] for item in inventories).items())),
        "relation_coverage": sorted(covered_relations),
        "formulation_class_coverage": sorted(covered_classes),
        "source_class_coverage": sorted(covered_sources),
        "polarity_coverage": sorted(covered_polarities),
        "accepted_disposition_coverage": sorted(covered_dispositions),
    }


def compile_manifest(manifest):
    """Compile privacy-safe corpus inventory without reviewed formulations."""
    inventory = validate_manifest(manifest)
    return {
        "schema": COMPILED_SCHEMA,
        "corpus_id": manifest["corpus_id"],
        "manifest_digest": inventory["manifest_digest"],
        "authority_change": "none",
        "persistence_change": "none",
        "raw_user_logs": False,
        "reviewed_formulations_only": True,
        "source_policy": "curated_synthetic_only",
        "cases_total": inventory["cases_total"],
        "category_distribution": inventory["category_distribution"],
        "relation_coverage": inventory["relation_coverage"],
        "formulation_class_coverage": inventory["formulation_class_coverage"],
        "source_class_coverage": inventory["source_class_coverage"],
        "polarity_coverage": inventory["polarity_coverage"],
        "accepted_disposition_coverage": inventory["accepted_disposition_coverage"],
        "parser_implementation": False,
        "runtime_integration": False,
        "promotion_decision": "none",
    }


def validate_actual_node(case_id, node):
    """Validate one typed parser prediction node."""
    reject_unknown_fields(
        f"{case_id}.prediction.node",
        node,
        {
            "node_id",
            "kind",
            "source",
            "polarity",
            "proposition",
            "confidence_basis_points",
            "parser_rule_id",
            "parser_rule_version",
        },
    )
    validate_id("prediction.node_id", node.get("node_id"))
    validate_id("prediction.parser_rule_id", node.get("parser_rule_id"))
    version = node.get("parser_rule_version")
    if not isinstance(version, int) or isinstance(version, bool) or not 1 <= version <= 65_535:
        raise ValidationError(f"{case_id}: invalid parser rule version")
    if node.get("kind") not in NODE_KINDS or node.get("source") not in SOURCE_CLASSES:
        raise ValidationError(f"{case_id}: invalid predicted node taxonomy")
    if node.get("polarity") not in POLARITIES:
        raise ValidationError(f"{case_id}: invalid predicted polarity")
    confidence = node.get("confidence_basis_points")
    if not isinstance(confidence, int) or isinstance(confidence, bool) or not 0 <= confidence <= 10_000:
        raise ValidationError(f"{case_id}: invalid predicted node confidence")
    validate_proposition(f"{case_id}.prediction.node.proposition", node.get("proposition"))


def validate_actual_relation(case_id, relation, node_ids):
    """Validate one typed parser prediction relation."""
    reject_unknown_fields(
        f"{case_id}.prediction.relation",
        relation,
        {
            "relation_id",
            "from",
            "to",
            "kind",
            "confidence_basis_points",
            "parser_rule_id",
            "parser_rule_version",
        },
    )
    validate_id("prediction.relation.relation_id", relation.get("relation_id"))
    source = relation.get("from")
    target = relation.get("to")
    validate_id("prediction.relation.from", source)
    validate_id("prediction.relation.to", target)
    if source not in node_ids or target not in node_ids:
        raise ValidationError(f"{case_id}: predicted relation has a dangling node")
    if source == target:
        raise ValidationError(f"{case_id}: predicted relation is a self-relation")
    if relation.get("kind") not in RELATION_KINDS:
        raise ValidationError(f"{case_id}: invalid predicted relation kind")
    confidence = relation.get("confidence_basis_points")
    if not isinstance(confidence, int) or isinstance(confidence, bool) or not 0 <= confidence <= 10_000:
        raise ValidationError(f"{case_id}: invalid predicted relation confidence")
    validate_id("prediction.relation.parser_rule_id", relation.get("parser_rule_id"))
    version = relation.get("parser_rule_version")
    if not isinstance(version, int) or isinstance(version, bool) or not 1 <= version <= 65_535:
        raise ValidationError(f"{case_id}: invalid relation rule version")


def validate_prediction_case(case):
    """Validate one prediction case, graph, parity evidence, and disposition."""
    reject_unknown_fields(
        "prediction case",
        case,
        {
            "case_id",
            "disposition",
            "nodes",
            "relations",
            "omissions",
            "receipt_digest",
            "replay_receipt_digest",
            "baseline_output_digest",
            "observed_output_digest",
            "replay_output_digest",
            "baseline_state_digest",
            "observed_state_digest",
            "replay_state_digest",
            "artifact",
        },
    )
    case_id = case.get("case_id")
    validate_id("prediction.case_id", case_id)
    if case.get("disposition") not in DISPOSITIONS:
        raise ValidationError(f"{case_id}: invalid prediction disposition")
    for field in (
        "receipt_digest",
        "replay_receipt_digest",
        "baseline_output_digest",
        "observed_output_digest",
        "replay_output_digest",
        "baseline_state_digest",
        "observed_state_digest",
        "replay_state_digest",
    ):
        validate_sha256(f"{case_id}.{field}", case.get(field))
    if not isinstance(case.get("artifact"), (dict, list)):
        raise ValidationError(f"{case_id}: artifact must be structured JSON")
    nodes = case.get("nodes")
    relations = case.get("relations")
    omissions = case.get("omissions")
    if not isinstance(nodes, list) or len(nodes) > 16:
        raise ValidationError(f"{case_id}: invalid predicted nodes")
    if not isinstance(relations, list) or len(relations) > 32:
        raise ValidationError(f"{case_id}: invalid predicted relations")
    if not isinstance(omissions, list) or len(omissions) > 16:
        raise ValidationError(f"{case_id}: invalid predicted omissions")
    if any(value not in OMISSION_REASONS for value in omissions):
        raise ValidationError(f"{case_id}: unsupported predicted omission")
    node_ids = set()
    for node in nodes:
        validate_actual_node(case_id, node)
        if node["node_id"] in node_ids:
            raise ValidationError(f"{case_id}: duplicate predicted node")
        node_ids.add(node["node_id"])
    relation_tuples = set()
    relation_ids = set()
    for relation in relations:
        validate_actual_relation(case_id, relation, node_ids)
        if relation["relation_id"] in relation_ids:
            raise ValidationError(f"{case_id}: duplicate predicted relation ID")
        relation_ids.add(relation["relation_id"])
        relation_tuple = (relation["from"], relation["to"], relation["kind"])
        if relation_tuple in relation_tuples:
            raise ValidationError(f"{case_id}: duplicate predicted relation")
        relation_tuples.add(relation_tuple)
    disposition = case["disposition"]
    if disposition == "parsed" and (not nodes or omissions):
        raise ValidationError(f"{case_id}: parsed prediction violates disposition semantics")
    if disposition == "partial" and (not nodes or not omissions):
        raise ValidationError(f"{case_id}: partial prediction violates disposition semantics")
    if disposition == "abstained" and (nodes or relations or not omissions):
        raise ValidationError(f"{case_id}: abstained prediction violates disposition semantics")
    return case


def validate_predictions(predictions, corpus_id, expected_case_ids):
    """Validate an exact-build prediction envelope against the gold case set."""
    reject_unknown_fields(
        "predictions",
        predictions,
        {"schema", "corpus_id", "build_sha", "authority_change", "raw_user_logs", "cases"},
    )
    if predictions.get("schema") != PREDICTIONS_SCHEMA:
        raise ValidationError("unsupported predictions schema")
    if predictions.get("corpus_id") != corpus_id:
        raise ValidationError("predictions corpus_id mismatch")
    if not isinstance(predictions.get("build_sha"), str) or not BUILD_SHA_PATTERN.fullmatch(
        predictions["build_sha"]
    ):
        raise ValidationError("predictions build_sha must be an exact Git SHA")
    if predictions.get("authority_change") != "none" or predictions.get("raw_user_logs") is not False:
        raise ValidationError("predictions cannot change authority or retain raw logs")
    cases = predictions.get("cases")
    if not isinstance(cases, list):
        raise ValidationError("prediction cases must be a list")
    validated = [validate_prediction_case(case) for case in cases]
    case_ids = [case["case_id"] for case in validated]
    if len(case_ids) != len(set(case_ids)):
        raise ValidationError("predictions contain duplicate case IDs")
    if set(case_ids) != set(expected_case_ids):
        raise ValidationError("predictions do not cover the exact gold case set")
    return {case["case_id"]: case for case in validated}


def forbidden_keys(value):
    """Find recursively forbidden privacy-bearing artifact keys."""
    found = set()
    if isinstance(value, dict):
        found.update(FORBIDDEN_ARTIFACT_KEYS.intersection(value))
        for child in value.values():
            found.update(forbidden_keys(child))
    elif isinstance(value, list):
        for child in value:
            found.update(forbidden_keys(child))
    return found


def node_signature(node):
    """Return the ID-independent semantic signature of a node."""
    return (
        node["kind"],
        node["source"],
        node["polarity"],
        json.dumps(node["proposition"], ensure_ascii=False, sort_keys=True, separators=(",", ":")),
    )


def relation_signatures(nodes, relations):
    """Return ID-independent semantic relation signatures with multiplicity."""
    signatures = {node["node_id"]: node_signature(node) for node in nodes}
    return collections.Counter(
        (signatures[relation["from"]], signatures[relation["to"]], relation["kind"])
        for relation in relations
    )


def confidence_floor_misses(expected_items, actual_items, signature, expected_field, actual_field):
    """Count matched predictions whose confidence is below the gold floor."""
    expected_by_signature = collections.defaultdict(list)
    actual_by_signature = collections.defaultdict(list)
    for item in expected_items:
        expected_by_signature[signature(item)].append(item[expected_field])
    for item in actual_items:
        actual_by_signature[signature(item)].append(item[actual_field])
    misses = 0
    for item_signature in expected_by_signature.keys() & actual_by_signature.keys():
        expected_values = sorted(expected_by_signature[item_signature], reverse=True)
        actual_values = sorted(actual_by_signature[item_signature], reverse=True)
        misses += sum(
            actual < expected
            for expected, actual in zip(expected_values, actual_values, strict=False)
        )
    return misses


def confidence_bucket(value):
    """Map basis-point confidence into a stable calibration bucket."""
    if value < 2_500:
        return "0000_2499"
    if value < 5_000:
        return "2500_4999"
    if value < 7_500:
        return "5000_7499"
    return "7500_10000"


def ratio_basis_points(numerator, denominator):
    """Calculate a bounded ratio or preserve an undefined denominator."""
    return None if denominator == 0 else numerator * 10_000 // denominator


def evaluate(manifest, predictions):
    """Score typed predictions and produce deterministic observation evidence."""
    inventory = validate_manifest(manifest)
    gold_by_id = {case["case_id"]: case for case in manifest["cases"]}
    predicted_by_id = validate_predictions(predictions, manifest["corpus_id"], gold_by_id)
    relation_counts = {
        kind: {"true_positive": 0, "false_positive": 0, "false_negative": 0}
        for kind in sorted(RELATION_KINDS)
    }
    node_counts = {
        kind: {"true_positive": 0, "false_positive": 0, "false_negative": 0}
        for kind in sorted(NODE_KINDS)
    }
    confidence = collections.defaultdict(
        lambda: collections.defaultdict(lambda: collections.Counter({"correct": 0, "incorrect": 0}))
    )
    abstentions = 0
    accepted_abstentions = 0
    unexpected_abstentions = 0
    disposition_mismatches = 0
    omission_mismatches = 0
    replay_failures = 0
    digest_mismatches = 0
    output_parity_violations = 0
    state_parity_violations = 0
    privacy_violations = 0
    confidence_floor_failures = 0

    for case_id in sorted(gold_by_id):
        gold = gold_by_id[case_id]
        predicted = predicted_by_id[case_id]
        if predicted["disposition"] == "abstained":
            abstentions += 1
            if "abstained" in gold["accepted_dispositions"]:
                accepted_abstentions += 1
            else:
                unexpected_abstentions += 1
        if predicted["disposition"] not in gold["accepted_dispositions"]:
            disposition_mismatches += 1
        if collections.Counter(predicted["omissions"]) != collections.Counter(gold["expected_omissions"]):
            omission_mismatches += 1

        expected_nodes = collections.Counter(node_signature(node) for node in gold["expected_nodes"])
        actual_nodes = collections.Counter(node_signature(node) for node in predicted["nodes"])
        for kind in NODE_KINDS:
            expected_kind = collections.Counter(
                {signature: count for signature, count in expected_nodes.items() if signature[0] == kind}
            )
            actual_kind = collections.Counter(
                {signature: count for signature, count in actual_nodes.items() if signature[0] == kind}
            )
            true_positive = sum((expected_kind & actual_kind).values())
            node_counts[kind]["true_positive"] += true_positive
            node_counts[kind]["false_positive"] += sum((actual_kind - expected_kind).values())
            node_counts[kind]["false_negative"] += sum((expected_kind - actual_kind).values())
        confidence_floor_failures += confidence_floor_misses(
            gold["expected_nodes"],
            predicted["nodes"],
            node_signature,
            "confidence_min_basis_points",
            "confidence_basis_points",
        )

        expected_relations = relation_signatures(gold["expected_nodes"], gold["expected_relations"])
        actual_relations = relation_signatures(predicted["nodes"], predicted["relations"])
        for kind in RELATION_KINDS:
            expected_kind = collections.Counter(
                {signature: count for signature, count in expected_relations.items() if signature[2] == kind}
            )
            actual_kind = collections.Counter(
                {signature: count for signature, count in actual_relations.items() if signature[2] == kind}
            )
            true_positive = sum((expected_kind & actual_kind).values())
            relation_counts[kind]["true_positive"] += true_positive
            relation_counts[kind]["false_positive"] += sum((actual_kind - expected_kind).values())
            relation_counts[kind]["false_negative"] += sum((expected_kind - actual_kind).values())

        expected_node_signatures = {
            node["node_id"]: node_signature(node) for node in gold["expected_nodes"]
        }
        actual_node_signatures = {
            node["node_id"]: node_signature(node) for node in predicted["nodes"]
        }
        def expected_relation_signature(relation, signatures=expected_node_signatures):
            return (signatures[relation["from"]], signatures[relation["to"]], relation["kind"])

        def actual_relation_signature(relation, signatures=actual_node_signatures):
            return (signatures[relation["from"]], signatures[relation["to"]], relation["kind"])
        confidence_floor_failures += confidence_floor_misses(
            gold["expected_relations"],
            predicted["relations"],
            expected_relation_signature,
            "confidence_min_basis_points",
            "confidence_basis_points",
        )

        expected_relation_set = set(expected_relations)
        actual_nodes = {node["node_id"]: node for node in predicted["nodes"]}
        for relation in predicted["relations"]:
            signature = (
                node_signature(actual_nodes[relation["from"]]),
                node_signature(actual_nodes[relation["to"]]),
                relation["kind"],
            )
            outcome = "correct" if signature in expected_relation_set else "incorrect"
            confidence[relation["kind"]][confidence_bucket(relation["confidence_basis_points"])][
                outcome
            ] += 1

        if predicted["receipt_digest"] != predicted["replay_receipt_digest"]:
            replay_failures += 1
            digest_mismatches += 1
        if len(
            {
                predicted["baseline_output_digest"],
                predicted["observed_output_digest"],
                predicted["replay_output_digest"],
            }
        ) != 1:
            output_parity_violations += 1
        if len(
            {
                predicted["baseline_state_digest"],
                predicted["observed_state_digest"],
                predicted["replay_state_digest"],
            }
        ) != 1:
            state_parity_violations += 1
        encoded_artifact = json.dumps(predicted, ensure_ascii=False, sort_keys=True)
        privacy_values = [gold["formulation"], *gold["privacy_needles"]]
        if forbidden_keys(predicted) or any(
            value and value.casefold() in encoded_artifact.casefold() for value in privacy_values
        ):
            privacy_violations += 1

    relation_metrics = {}
    for kind, counts in sorted(relation_counts.items()):
        precision_denominator = counts["true_positive"] + counts["false_positive"]
        recall_denominator = counts["true_positive"] + counts["false_negative"]
        relation_metrics[kind] = {
            **counts,
            "precision_basis_points": ratio_basis_points(counts["true_positive"], precision_denominator),
            "recall_basis_points": ratio_basis_points(counts["true_positive"], recall_denominator),
        }
    node_metrics = {}
    for kind, counts in sorted(node_counts.items()):
        precision_denominator = counts["true_positive"] + counts["false_positive"]
        recall_denominator = counts["true_positive"] + counts["false_negative"]
        node_metrics[kind] = {
            **counts,
            "precision_basis_points": ratio_basis_points(counts["true_positive"], precision_denominator),
            "recall_basis_points": ratio_basis_points(counts["true_positive"], recall_denominator),
        }

    report = {
        "schema": REPORT_SCHEMA,
        "corpus_id": manifest["corpus_id"],
        "manifest_digest": inventory["manifest_digest"],
        "build_sha": predictions["build_sha"],
        "authority_change": "none",
        "persistence_change": "none",
        "promotion_decision": "none",
        "cases_total": len(gold_by_id),
        "node_metrics": node_metrics,
        "relation_metrics": relation_metrics,
        "abstentions": abstentions,
        "abstention_rate_basis_points": ratio_basis_points(abstentions, len(gold_by_id)),
        "accepted_abstentions": accepted_abstentions,
        "unexpected_abstentions": unexpected_abstentions,
        "disposition_mismatches": disposition_mismatches,
        "omission_mismatches": omission_mismatches,
        "confidence_floor_failures": confidence_floor_failures,
        "confidence_calibration": {
            kind: {
                bucket: dict(sorted(values.items()))
                for bucket, values in sorted(confidence[kind].items())
            }
            for kind in sorted(RELATION_KINDS)
        },
        # Structural validation and graph-reference failures raise before a
        # report exists. A retained report therefore records these as zero by
        # construction rather than as recoverable evaluation outcomes.
        "validation_failures": 0,
        "replay_failures": replay_failures,
        "privacy_violations": privacy_violations,
        "output_parity_violations": output_parity_violations,
        "state_parity_violations": state_parity_violations,
        "digest_mismatches": digest_mismatches,
        "invalid_graph_references": 0,
    }
    report["zero_failure_budgets_met"] = all(
        report[field] == 0
        for field in (
            "validation_failures",
            "replay_failures",
            "privacy_violations",
            "output_parity_violations",
            "state_parity_violations",
            "digest_mismatches",
            "invalid_graph_references",
        )
    )
    report["report_digest"] = canonical_digest(report, REPORT_DOMAIN)
    return report


def write_new_json(path, value):
    """Write formatted JSON with create-new semantics."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x", encoding="utf-8") as output:
        json.dump(value, output, ensure_ascii=False, indent=2, sort_keys=True)
        output.write("\n")


def main():
    """Run the validation, digest, compile, or evaluation command."""
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate = subparsers.add_parser("validate")
    validate.add_argument("manifest", nargs="?", default=DEFAULT_MANIFEST)
    digest = subparsers.add_parser("digest")
    digest.add_argument("manifest", nargs="?", default=DEFAULT_MANIFEST)
    compile_command = subparsers.add_parser("compile")
    compile_command.add_argument("manifest", nargs="?", default=DEFAULT_MANIFEST)
    compile_command.add_argument("--output")
    evaluation = subparsers.add_parser("evaluate")
    evaluation.add_argument("--manifest", default=DEFAULT_MANIFEST)
    evaluation.add_argument("--predictions", required=True)
    evaluation.add_argument("--output")
    arguments = parser.parse_args()
    try:
        manifest = load_json(arguments.manifest)
        if arguments.command == "digest":
            result = {"manifest_digest": manifest_digest(manifest)}
        elif arguments.command == "validate":
            result = validate_manifest(manifest)
        elif arguments.command == "compile":
            result = compile_manifest(manifest)
        else:
            result = evaluate(manifest, load_json(arguments.predictions))
        if getattr(arguments, "output", None):
            write_new_json(arguments.output, result)
        print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    except (OSError, ValidationError) as error:
        parser.exit(1, f"user argument evaluation failed: {error}\n")


if __name__ == "__main__":
    main()
