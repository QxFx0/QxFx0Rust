//! Bounded, canonical metadata for thematic pack discovery and activation.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const CATALOG_SCHEMA_VERSION: u32 = 1;
pub const MAX_CATALOG_PACKS: usize = 64;
const MAX_ITEMS: usize = 1024;
const MAX_TEXT: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackTrustTier {
    CuratedEmbedded,
    ReviewedExternal,
    DiscoveryOnly,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackLifecycle {
    Approved,
    Candidate,
    Planned,
    Retired,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackPin {
    pub pack_id: String,
    pub pack_version: u32,
    pub manifest_digest: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackOwnership {
    #[serde(default)]
    pub concept_namespaces: Vec<String>,
    #[serde(default)]
    pub thesis_namespaces: Vec<String>,
    #[serde(default)]
    pub overlay_of: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPack {
    pub pack_id: String,
    pub pack_version: u32,
    pub manifest_digest: String,
    pub themes: Vec<String>,
    #[serde(default)]
    pub concept_coverage: Vec<String>,
    #[serde(default)]
    pub thesis_coverage: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<PackPin>,
    pub trust_tier: PackTrustTier,
    pub lifecycle: PackLifecycle,
    pub ownership: PackOwnership,
    pub license: String,
    pub authority_facts: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackCatalog {
    pub schema_version: u32,
    pub catalog_id: String,
    pub catalog_version: u32,
    pub catalog_digest: String,
    pub packs: Vec<CatalogPack>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogManifest {
    pub schema_version: u32,
    pub catalog_id: String,
    pub catalog_version: u32,
    pub catalog_digest: String,
    pub files: BTreeMap<String, String>,
    pub manifest_digest: String,
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    #[error("catalog validation failed: {0}")]
    Validation(String),
    #[error("catalog digest mismatch")]
    CatalogDigestMismatch,
    #[error("catalog manifest digest mismatch")]
    ManifestDigestMismatch,
}

fn digest<T: Serialize>(value: &T) -> Result<String, CatalogError> {
    let bytes = serde_json::to_vec(value).map_err(|e| CatalogError::Validation(e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn text(name: &str, value: &str) -> Result<(), CatalogError> {
    if value.is_empty() || value.len() > MAX_TEXT {
        Err(CatalogError::Validation(format!("invalid {name}")))
    } else {
        Ok(())
    }
}
fn hex_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn unique_bounded(
    name: &str,
    values: &mut [String],
    allow_empty: bool,
) -> Result<(), CatalogError> {
    if values.len() > MAX_ITEMS || (!allow_empty && values.is_empty()) {
        return Err(CatalogError::Validation(format!("invalid {name} census")));
    }
    for value in values.iter() {
        text(name, value)?;
    }
    values.sort();
    if values.windows(2).any(|w| w[0] == w[1]) {
        return Err(CatalogError::Validation(format!("duplicate {name}")));
    }
    Ok(())
}
impl PackCatalog {
    fn canonical_without_digest(&self) -> Result<Self, CatalogError> {
        let mut out = self.clone();
        out.catalog_digest.clear();
        out.packs.sort_by(|a, b| a.pack_id.cmp(&b.pack_id));
        for p in &mut out.packs {
            p.themes.sort();
            p.concept_coverage.sort();
            p.thesis_coverage.sort();
            p.dependencies.sort();
            p.ownership.concept_namespaces.sort();
            p.ownership.thesis_namespaces.sort();
            p.ownership.overlay_of.sort();
        }
        Ok(out)
    }
    pub fn canonical_digest(&self) -> Result<String, CatalogError> {
        digest(&self.canonical_without_digest()?)
    }
    pub fn validate(&self) -> Result<(), CatalogError> {
        if self.schema_version != CATALOG_SCHEMA_VERSION
            || self.catalog_version == 0
            || self.packs.is_empty()
            || self.packs.len() > MAX_CATALOG_PACKS
        {
            return Err(CatalogError::Validation(
                "invalid catalog identity or census".into(),
            ));
        }
        text("catalog_id", &self.catalog_id)?;
        if !hex_digest(&self.catalog_digest) || self.canonical_digest()? != self.catalog_digest {
            return Err(CatalogError::CatalogDigestMismatch);
        }
        let mut ids = BTreeSet::new();
        let mut pins = BTreeMap::new();
        for p0 in &self.packs {
            let mut p = p0.clone();
            text("pack_id", &p.pack_id)?;
            if p.pack_version == 0
                || !hex_digest(&p.manifest_digest)
                || p.license.is_empty()
                || !ids.insert(p.pack_id.clone())
            {
                return Err(CatalogError::Validation(
                    "invalid or duplicate pack identity".into(),
                ));
            }
            unique_bounded("theme", &mut p.themes, false)?;
            unique_bounded("concept coverage", &mut p.concept_coverage, true)?;
            unique_bounded("thesis coverage", &mut p.thesis_coverage, true)?;
            unique_bounded(
                "concept namespace",
                &mut p.ownership.concept_namespaces,
                true,
            )?;
            unique_bounded("thesis namespace", &mut p.ownership.thesis_namespaces, true)?;
            unique_bounded("overlay owner", &mut p.ownership.overlay_of, true)?;
            if !matches!(p.lifecycle, PackLifecycle::Approved) && p.authority_facts {
                return Err(CatalogError::Validation(
                    "candidate/planned/retired pack cannot contain authority facts".into(),
                ));
            }
            if matches!(p.trust_tier, PackTrustTier::DiscoveryOnly) && p.authority_facts {
                return Err(CatalogError::Validation(
                    "discovery is not authority".into(),
                ));
            }
            pins.insert(
                p.pack_id.clone(),
                (p.pack_version, p.manifest_digest.clone()),
            );
        }
        let mut edges = BTreeMap::new();
        let mut concept_owner: BTreeMap<String, String> = BTreeMap::new();
        let mut thesis_owner: BTreeMap<String, String> = BTreeMap::new();
        for p in &self.packs {
            let mut deps = BTreeSet::new();
            for d in &p.dependencies {
                if d.pack_id == p.pack_id
                    || !deps.insert(d.pack_id.clone())
                    || pins.get(&d.pack_id) != Some(&(d.pack_version, d.manifest_digest.clone()))
                {
                    return Err(CatalogError::Validation(
                        "missing, duplicate, or unpinned dependency".into(),
                    ));
                }
            }
            edges.insert(p.pack_id.clone(), deps);
            for (ns, owners) in [
                (&p.ownership.concept_namespaces, &mut concept_owner),
                (&p.ownership.thesis_namespaces, &mut thesis_owner),
            ] {
                for n in ns {
                    if let Some(owner) = owners.insert(n.clone(), p.pack_id.clone()) {
                        if !p.ownership.overlay_of.contains(&owner)
                            || !p.dependencies.iter().any(|d| d.pack_id == owner)
                        {
                            return Err(CatalogError::Validation(format!(
                                "ownership collision for {n}"
                            )));
                        }
                    }
                }
            }
            for owner in &p.ownership.overlay_of {
                if !ids.contains(owner) || !p.dependencies.iter().any(|d| &d.pack_id == owner) {
                    return Err(CatalogError::Validation(
                        "overlay owner must be a dependency".into(),
                    ));
                }
            }
        }
        fn visit(
            n: &str,
            e: &BTreeMap<String, BTreeSet<String>>,
            temp: &mut BTreeSet<String>,
            done: &mut BTreeSet<String>,
        ) -> bool {
            if done.contains(n) {
                return true;
            }
            if !temp.insert(n.into()) {
                return false;
            }
            for d in &e[n] {
                if !visit(d, e, temp, done) {
                    return false;
                }
            }
            temp.remove(n);
            done.insert(n.into());
            true
        }
        let mut done = BTreeSet::new();
        for id in &ids {
            if !visit(id, &edges, &mut BTreeSet::new(), &mut done) {
                return Err(CatalogError::Validation("cyclic dependencies".into()));
            }
        }
        Ok(())
    }
}
impl CatalogManifest {
    fn canonical_without_digest(&self) -> Self {
        let mut x = self.clone();
        x.manifest_digest.clear();
        x
    }
    pub fn canonical_digest(&self) -> Result<String, CatalogError> {
        digest(&self.canonical_without_digest())
    }
    pub fn validate(
        &self,
        catalog_bytes: &[u8],
        catalog: &PackCatalog,
    ) -> Result<(), CatalogError> {
        if self.schema_version != CATALOG_SCHEMA_VERSION
            || self.catalog_id != catalog.catalog_id
            || self.catalog_version != catalog.catalog_version
            || self.catalog_digest != catalog.catalog_digest
            || self.files.len() != 1
            || self.files.get("catalog.json")
                != Some(&format!("{:x}", Sha256::digest(catalog_bytes)))
        {
            return Err(CatalogError::Validation(
                "catalog manifest binding mismatch".into(),
            ));
        }
        if !hex_digest(&self.manifest_digest) || self.canonical_digest()? != self.manifest_digest {
            return Err(CatalogError::ManifestDigestMismatch);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn load() -> PackCatalog {
        serde_json::from_slice(include_bytes!("../../data/packs/catalog-v1/catalog.json")).unwrap()
    }
    fn resign(c: &mut PackCatalog) {
        c.catalog_digest = c.canonical_digest().unwrap();
    }
    #[test]
    fn catalog_census_and_order_invariance() {
        let c = load();
        c.validate().unwrap();
        assert_eq!(c.packs.len(), 9);
        assert_eq!(
            c.packs
                .iter()
                .filter(|p| p.lifecycle == PackLifecycle::Approved)
                .count(),
            4
        );
        assert_eq!(
            c.packs
                .iter()
                .filter(|p| matches!(
                    p.lifecycle,
                    PackLifecycle::Candidate | PackLifecycle::Planned
                ))
                .count(),
            5
        );
        let mut r = c.clone();
        r.packs.reverse();
        assert_eq!(c.canonical_digest().unwrap(), r.canonical_digest().unwrap());
    }
    #[test]
    fn tampering_is_detected() {
        let mut c = load();
        c.packs[0].themes.push("tampered".into());
        assert_eq!(c.validate(), Err(CatalogError::CatalogDigestMismatch));
    }
    #[test]
    fn missing_dependency_pin_fails() {
        let mut c = load();
        let p = c
            .packs
            .iter_mut()
            .find(|p| p.pack_id == "agency-responsibility-v1")
            .unwrap();
        p.dependencies[0].manifest_digest = "0".repeat(64);
        resign(&mut c);
        assert!(matches!(c.validate(), Err(CatalogError::Validation(_))));
    }
    #[test]
    fn cyclic_dependencies_fail() {
        let mut c = load();
        let overlay = c
            .packs
            .iter()
            .find(|p| p.pack_id == "agency-responsibility-v1")
            .unwrap();
        let pin = PackPin {
            pack_id: overlay.pack_id.clone(),
            pack_version: overlay.pack_version,
            manifest_digest: overlay.manifest_digest.clone(),
        };
        c.packs
            .iter_mut()
            .find(|p| p.pack_id == "philosophy-core-v1")
            .unwrap()
            .dependencies
            .push(pin);
        resign(&mut c);
        assert!(matches!(c.validate(), Err(CatalogError::Validation(_))));
    }
    #[test]
    fn ownership_collision_fails() {
        let mut c = load();
        c.packs
            .iter_mut()
            .find(|p| p.pack_id == "agency-responsibility-v1")
            .unwrap()
            .ownership
            .concept_namespaces
            .push("concept.".into());
        resign(&mut c);
        assert!(matches!(c.validate(), Err(CatalogError::Validation(_))));
    }
}
