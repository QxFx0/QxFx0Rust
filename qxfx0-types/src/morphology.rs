use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Source tier for lexeme provenance, ordered by trust level.
/// Higher trust tiers are preferred during candidate resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Default)]
pub enum SourceTier {
    /// Manually curated, verified entries
    Curated,
    /// Human-reviewed entries
    Reviewed,
    /// Automatically verified entries
    AutoVerified,
    /// Automatically generated coverage entries
    #[default]
    AutoCoverage,
}

impl SourceTier {
    /// Return numeric trust rank. Higher is more trusted.
    pub fn trust_rank(&self) -> u8 {
        match self {
            SourceTier::Curated => 4,
            SourceTier::Reviewed => 3,
            SourceTier::AutoVerified => 2,
            SourceTier::AutoCoverage => 1,
        }
    }
}

impl<'de> Deserialize<'de> for SourceTier {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "curated" => Ok(SourceTier::Curated),
            "reviewed" => Ok(SourceTier::Reviewed),
            "auto_verified" => Ok(SourceTier::AutoVerified),
            "auto_coverage" => Ok(SourceTier::AutoCoverage),
            _ => Err(serde::de::Error::custom(format!(
                "Unknown source tier: {}",
                s
            ))),
        }
    }
}

use std::str::FromStr;

/// Part of speech for Russian lexemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum PartOfSpeech {
    #[default]
    Noun,
    Adjective,
    Verb,
    Adverb,
    Pronoun,
    Preposition,
    Conjunction,
    Interjection,
    Particle,
    Numeral,
    Other,
}

impl FromStr for PartOfSpeech {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "noun" => PartOfSpeech::Noun,
            "adjective" => PartOfSpeech::Adjective,
            "verb" => PartOfSpeech::Verb,
            "adverb" => PartOfSpeech::Adverb,
            "pronoun" => PartOfSpeech::Pronoun,
            "preposition" => PartOfSpeech::Preposition,
            "conjunction" => PartOfSpeech::Conjunction,
            "interjection" => PartOfSpeech::Interjection,
            "particle" => PartOfSpeech::Particle,
            "numeral" => PartOfSpeech::Numeral,
            _ => PartOfSpeech::Other,
        })
    }
}

/// Gender for Russian nouns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Gender {
    Masculine,
    Feminine,
    Neuter,
    #[default]
    Unknown,
}

impl FromStr for Gender {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "masculine" | "masc" => Gender::Masculine,
            "feminine" | "femn" | "fem" => Gender::Feminine,
            "neuter" | "neut" => Gender::Neuter,
            _ => Gender::Unknown,
        })
    }
}

impl Gender {
    pub fn as_str(&self) -> &'static str {
        match self {
            Gender::Masculine => "masculine",
            Gender::Feminine => "feminine",
            Gender::Neuter => "neuter",
            Gender::Unknown => "unknown",
        }
    }
}

/// Animacy for Russian nouns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Animacy {
    Animate,
    Inanimate,
    #[default]
    Unknown,
}

impl FromStr for Animacy {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "animate" | "anim" => Animacy::Animate,
            "inanimate" | "inan" => Animacy::Inanimate,
            _ => Animacy::Unknown,
        })
    }
}

impl Animacy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Animacy::Animate => "animate",
            Animacy::Inanimate => "inanimate",
            Animacy::Unknown => "unknown",
        }
    }
}

/// Grammatical number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Number {
    Singular,
    Plural,
}

impl Number {
    pub fn as_str(&self) -> &'static str {
        match self {
            Number::Singular => "sg",
            Number::Plural => "pl",
        }
    }
}

/// Grammatical case for Russian.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Case {
    Nominative,
    Genitive,
    Dative,
    Accusative,
    Instrumental,
    Prepositional,
}

impl Case {
    pub fn as_str(&self) -> &'static str {
        match self {
            Case::Nominative => "nom",
            Case::Genitive => "gen",
            Case::Dative => "dat",
            Case::Accusative => "acc",
            Case::Instrumental => "ins",
            Case::Prepositional => "prep",
        }
    }
}

impl FromStr for Case {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "nominative" | "nom" => Case::Nominative,
            "genitive" | "gen" => Case::Genitive,
            "dative" | "dat" => Case::Dative,
            "accusative" | "acc" => Case::Accusative,
            "instrumental" | "ins" => Case::Instrumental,
            "prepositional" | "prep" | "loc" => Case::Prepositional,
            _ => Case::Nominative,
        })
    }
}

/// Combined case and number for form lookup.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaseNumber {
    pub case: Case,
    pub number: Number,
}

impl CaseNumber {
    pub fn new(case: Case, number: Number) -> Self {
        Self { case, number }
    }

    pub fn as_key(&self) -> String {
        format!("{}_{}", self.case.as_str(), self.number.as_str())
    }

    pub fn from_key(key: &str) -> Option<Self> {
        let parts: Vec<&str> = key.split('_').collect();
        if parts.len() != 2 {
            return None;
        }
        let case = Case::from_str(parts[0]).ok()?;
        let number = match parts[1] {
            "sg" => Number::Singular,
            "pl" => Number::Plural,
            _ => return None,
        };
        Some(Self { case, number })
    }
}

/// Grammar features for a lexeme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GrammarFeatures {
    pub pos: PartOfSpeech,
    pub gender: Gender,
    pub animacy: Animacy,
}

/// A complete set of inflected forms for a lexeme.
/// Contains all 12 forms: 6 cases x 2 numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct InflectionForms {
    #[serde(default)]
    pub nom_sg: String,
    #[serde(default)]
    pub nom_pl: String,
    #[serde(default)]
    pub gen_sg: String,
    #[serde(default)]
    pub gen_pl: String,
    #[serde(default)]
    pub dat_sg: String,
    #[serde(default)]
    pub dat_pl: String,
    #[serde(default)]
    pub acc_sg: String,
    #[serde(default)]
    pub acc_pl: String,
    #[serde(default)]
    pub ins_sg: String,
    #[serde(default)]
    pub ins_pl: String,
    #[serde(default)]
    pub prep_sg: String,
    #[serde(default)]
    pub prep_pl: String,
}

impl InflectionForms {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get form for a specific case and number.
    pub fn get(&self, case: Case, number: Number) -> &str {
        match (case, number) {
            (Case::Nominative, Number::Singular) => &self.nom_sg,
            (Case::Nominative, Number::Plural) => &self.nom_pl,
            (Case::Genitive, Number::Singular) => &self.gen_sg,
            (Case::Genitive, Number::Plural) => &self.gen_pl,
            (Case::Dative, Number::Singular) => &self.dat_sg,
            (Case::Dative, Number::Plural) => &self.dat_pl,
            (Case::Accusative, Number::Singular) => &self.acc_sg,
            (Case::Accusative, Number::Plural) => &self.acc_pl,
            (Case::Instrumental, Number::Singular) => &self.ins_sg,
            (Case::Instrumental, Number::Plural) => &self.ins_pl,
            (Case::Prepositional, Number::Singular) => &self.prep_sg,
            (Case::Prepositional, Number::Plural) => &self.prep_pl,
        }
    }

    /// Set form for a specific case and number.
    pub fn set(&mut self, case: Case, number: Number, form: String) {
        match (case, number) {
            (Case::Nominative, Number::Singular) => self.nom_sg = form,
            (Case::Nominative, Number::Plural) => self.nom_pl = form,
            (Case::Genitive, Number::Singular) => self.gen_sg = form,
            (Case::Genitive, Number::Plural) => self.gen_pl = form,
            (Case::Dative, Number::Singular) => self.dat_sg = form,
            (Case::Dative, Number::Plural) => self.dat_pl = form,
            (Case::Accusative, Number::Singular) => self.acc_sg = form,
            (Case::Accusative, Number::Plural) => self.acc_pl = form,
            (Case::Instrumental, Number::Singular) => self.ins_sg = form,
            (Case::Instrumental, Number::Plural) => self.ins_pl = form,
            (Case::Prepositional, Number::Singular) => self.prep_sg = form,
            (Case::Prepositional, Number::Plural) => self.prep_pl = form,
        }
    }

    /// Convert from a partial map (as found in Haskell JSON).
    /// Missing forms are filled with the nominative singular.
    pub fn from_partial_map(map: &BTreeMap<String, String>, nom_sg: &str) -> Self {
        let mut forms = Self::new();

        // Set nominative singular
        forms.nom_sg = nom_sg.to_string();
        forms.nom_pl = map
            .get("NomPl")
            .cloned()
            .unwrap_or_else(|| nom_sg.to_string());

        // Set all other forms from map or default to nom_sg
        for (key, value) in map {
            let key_upper = key.to_uppercase();
            match key_upper.as_str() {
                "NOMSG" => forms.nom_sg = value.clone(),
                "NOMPL" => forms.nom_pl = value.clone(),
                "GENSG" => forms.gen_sg = value.clone(),
                "GENPL" => forms.gen_pl = value.clone(),
                "DATSG" => forms.dat_sg = value.clone(),
                "DATP" | "DATPL" => forms.dat_pl = value.clone(),
                "ACCSG" => forms.acc_sg = value.clone(),
                "ACCPL" => forms.acc_pl = value.clone(),
                "INSSG" => forms.ins_sg = value.clone(),
                "INSP" | "INSPL" => forms.ins_pl = value.clone(),
                "LOCSG" | "PREPSG" => forms.prep_sg = value.clone(),
                "LOCPL" | "PREPPL" => forms.prep_pl = value.clone(),
                _ => {}
            }
        }

        // Fill missing forms with nom_sg as fallback
        if forms.gen_sg.is_empty() {
            forms.gen_sg = nom_sg.to_string();
        }
        if forms.dat_sg.is_empty() {
            forms.dat_sg = nom_sg.to_string();
        }
        if forms.acc_sg.is_empty() {
            forms.acc_sg = nom_sg.to_string();
        }
        if forms.ins_sg.is_empty() {
            forms.ins_sg = nom_sg.to_string();
        }
        if forms.prep_sg.is_empty() {
            forms.prep_sg = nom_sg.to_string();
        }
        if forms.gen_pl.is_empty() {
            forms.gen_pl = forms.nom_pl.clone();
        }
        if forms.dat_pl.is_empty() {
            forms.dat_pl = forms.nom_pl.clone();
        }
        if forms.acc_pl.is_empty() {
            forms.acc_pl = forms.nom_pl.clone();
        }
        if forms.ins_pl.is_empty() {
            forms.ins_pl = forms.nom_pl.clone();
        }
        if forms.prep_pl.is_empty() {
            forms.prep_pl = forms.nom_pl.clone();
        }

        forms
    }

    /// Get all surface forms as a map from case_number key to form.
    pub fn to_map(&self) -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        map.insert("nom_sg".to_string(), self.nom_sg.clone());
        map.insert("nom_pl".to_string(), self.nom_pl.clone());
        map.insert("gen_sg".to_string(), self.gen_sg.clone());
        map.insert("gen_pl".to_string(), self.gen_pl.clone());
        map.insert("dat_sg".to_string(), self.dat_sg.clone());
        map.insert("dat_pl".to_string(), self.dat_pl.clone());
        map.insert("acc_sg".to_string(), self.acc_sg.clone());
        map.insert("acc_pl".to_string(), self.acc_pl.clone());
        map.insert("ins_sg".to_string(), self.ins_sg.clone());
        map.insert("ins_pl".to_string(), self.ins_pl.clone());
        map.insert("prep_sg".to_string(), self.prep_sg.clone());
        map.insert("prep_pl".to_string(), self.prep_pl.clone());
        map
    }
}

/// A lexeme entry in the morphology dictionary.
/// Represents a lemma with its grammatical features and all inflected forms.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LexemeEntry {
    /// The lemma in nominative singular form (canonical form)
    pub lemma: String,
    /// Grammatical features
    pub features: GrammarFeatures,
    /// All inflected forms
    pub forms: InflectionForms,
    /// Source tier for provenance
    pub source_tier: SourceTier,
    /// Quality score (0.0 to 1.0)
    pub quality: f64,
}

impl LexemeEntry {
    pub fn new(lemma: impl Into<String>) -> Self {
        Self {
            lemma: lemma.into(),
            features: GrammarFeatures::default(),
            forms: InflectionForms::new(),
            source_tier: SourceTier::default(),
            quality: 1.0,
        }
    }

    /// Get the form for a specific case and number.
    pub fn get_form(&self, case: Case, number: Number) -> &str {
        self.forms.get(case, number)
    }

    /// Check if this lexeme has complete inflection data.
    pub fn is_complete(&self) -> bool {
        !self.forms.nom_sg.is_empty()
            && !self.forms.gen_sg.is_empty()
            && !self.forms.dat_sg.is_empty()
            && !self.forms.acc_sg.is_empty()
            && !self.forms.ins_sg.is_empty()
            && !self.forms.prep_sg.is_empty()
    }
}

/// Flat entry format for JSON deserialization (without features object)
#[derive(Debug, Deserialize)]
struct FlatLexemeEntry {
    lemma: String,
    #[serde(default)]
    pos: String,
    #[serde(default)]
    gender: String,
    #[serde(default)]
    animacy: String,
    #[serde(default)]
    source_tier: SourceTier,
    #[serde(default)]
    quality: f64,
    forms: InflectionForms,
}

impl<'de> Deserialize<'de> for LexemeEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let flat: FlatLexemeEntry = FlatLexemeEntry::deserialize(deserializer)?;

        Ok(Self {
            lemma: flat.lemma,
            features: GrammarFeatures {
                pos: PartOfSpeech::from_str(&flat.pos).unwrap(),
                gender: Gender::from_str(&flat.gender).unwrap(),
                animacy: Animacy::from_str(&flat.animacy).unwrap(),
            },
            forms: flat.forms,
            source_tier: flat.source_tier,
            quality: flat.quality,
        })
    }
}

/// A candidate lexeme for a given surface form.
/// Used during morphology analysis when a surface form can map to multiple lemmas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LexemeCandidate {
    /// The surface form that was matched
    pub surface: String,
    /// The candidate lexeme entry
    pub entry: LexemeEntry,
    /// The case and number that produced this surface form
    pub case_number: CaseNumber,
    /// Confidence score for this candidate (0.0 to 1.0)
    pub confidence: f64,
}

impl LexemeCandidate {
    pub fn new(surface: impl Into<String>, entry: LexemeEntry, case_number: CaseNumber) -> Self {
        Self {
            surface: surface.into(),
            entry,
            case_number,
            confidence: 1.0,
        }
    }
}

/// Result of a morphology lookup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MorphologyLookup {
    /// Successfully resolved to a unique lexeme
    Resolved(LexemeResolution),
    /// Multiple candidates with same trust rank and quality
    Ambiguous(Vec<LexemeCandidate>),
    /// No matching lexeme found
    Unknown,
}

/// Detailed resolution of a surface form to its lemma and features.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LexemeResolution {
    pub lemma: String,
    pub surface: String,
    pub case: Case,
    pub number: Number,
    pub pos: PartOfSpeech,
    pub gender: Gender,
    pub animacy: Animacy,
    pub source_tier: SourceTier,
    pub quality: f64,
}

/// Morphology bundle manifest for asset provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MorphologyBundleManifest {
    pub bundle_version: u32,
    pub source_repository: String,
    pub source_commit: String,
    pub license: String,
    pub created_at: String,
    pub files: BTreeMap<String, String>,
}

impl MorphologyBundleManifest {
    pub fn new(source_commit: impl Into<String>) -> Self {
        Self {
            bundle_version: 1,
            source_repository: "QxFx0".to_string(),
            source_commit: source_commit.into(),
            license: "MIT".to_string(),
            created_at: String::new(),
            files: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_case_number_key_roundtrip() {
        let cn = CaseNumber::new(Case::Genitive, Number::Singular);
        let key = cn.as_key();
        assert_eq!(key, "gen_sg");
        let parsed = CaseNumber::from_key(&key).unwrap();
        assert_eq!(parsed.case, Case::Genitive);
        assert_eq!(parsed.number, Number::Singular);
    }

    #[test]
    fn test_grammar_features_default() {
        let features = GrammarFeatures::default();
        assert_eq!(features.pos, PartOfSpeech::Noun);
        assert_eq!(features.gender, Gender::Unknown);
        assert_eq!(features.animacy, Animacy::Unknown);
    }

    #[test]
    fn test_source_tier_trust_ranks() {
        assert_eq!(SourceTier::Curated.trust_rank(), 4);
        assert_eq!(SourceTier::Reviewed.trust_rank(), 3);
        assert_eq!(SourceTier::AutoVerified.trust_rank(), 2);
        assert_eq!(SourceTier::AutoCoverage.trust_rank(), 1);
    }

    #[test]
    fn test_trust_rank_total_ordering() {
        let tiers = [
            SourceTier::Curated,
            SourceTier::Reviewed,
            SourceTier::AutoVerified,
            SourceTier::AutoCoverage,
        ];
        for w in tiers.windows(2) {
            assert!(
                w[0].trust_rank() > w[1].trust_rank(),
                "trust_rank must be strictly decreasing across tiers"
            );
        }
    }

    #[test]
    fn test_inflection_forms_get() {
        let mut forms = InflectionForms::new();
        forms.nom_sg = "свобода".to_string();
        forms.gen_sg = "свободы".to_string();

        assert_eq!(forms.get(Case::Nominative, Number::Singular), "свобода");
        assert_eq!(forms.get(Case::Genitive, Number::Singular), "свободы");
    }

    #[test]
    fn test_lexeme_entry_is_complete() {
        let mut entry = LexemeEntry::new("свобода");
        entry.forms.nom_sg = "свобода".to_string();
        entry.forms.gen_sg = "свободы".to_string();
        entry.forms.dat_sg = "свободе".to_string();
        entry.forms.acc_sg = "свободу".to_string();
        entry.forms.ins_sg = "свободой".to_string();
        entry.forms.prep_sg = "свободе".to_string();

        assert!(entry.is_complete());
    }

    #[test]
    fn test_gender_from_str() {
        assert_eq!(Gender::from_str("masc").unwrap(), Gender::Masculine);
        assert_eq!(Gender::from_str("femn").unwrap(), Gender::Feminine);
        assert_eq!(Gender::from_str("neut").unwrap(), Gender::Neuter);
        assert_eq!(Gender::from_str("unknown").unwrap(), Gender::Unknown);
    }

    #[test]
    fn test_animacy_from_str() {
        assert_eq!(Animacy::from_str("anim").unwrap(), Animacy::Animate);
        assert_eq!(Animacy::from_str("inan").unwrap(), Animacy::Inanimate);
        assert_eq!(Animacy::from_str("unknown").unwrap(), Animacy::Unknown);
    }

    #[test]
    fn test_pos_from_str() {
        assert_eq!(PartOfSpeech::from_str("Noun").unwrap(), PartOfSpeech::Noun);
        assert_eq!(PartOfSpeech::from_str("noun").unwrap(), PartOfSpeech::Noun);
        assert_eq!(PartOfSpeech::from_str("verb").unwrap(), PartOfSpeech::Verb);
    }
}
