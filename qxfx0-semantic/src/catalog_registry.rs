//! Immutable discovery registry and fail-closed digest-pinned activation policy.

use qxfx0_types::{
    CatalogError, CatalogManifest, CatalogPack, PackCatalog, PackLifecycle, PackPin, PackTrustTier,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct CatalogRegistry {
    catalog: PackCatalog,
    by_id: BTreeMap<String, CatalogPack>,
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActivationError {
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error("pack is not catalogued: {0}")]
    Unknown(String),
    #[error("pack is not approved authority: {0}")]
    NotApproved(String),
    #[error("pack pin is not allowlisted: {0}")]
    NotAllowlisted(String),
    #[error("approved pack dependency is not selected: {pack} requires {dependency}")]
    MissingDependency { pack: String, dependency: String },
}
impl CatalogRegistry {
    pub fn load(catalog_bytes: &[u8], manifest_bytes: &[u8]) -> Result<Self, ActivationError> {
        let catalog: PackCatalog = serde_json::from_slice(catalog_bytes)
            .map_err(|e| CatalogError::Validation(e.to_string()))?;
        let manifest: CatalogManifest = serde_json::from_slice(manifest_bytes)
            .map_err(|e| CatalogError::Validation(e.to_string()))?;
        catalog.validate()?;
        manifest.validate(catalog_bytes, &catalog)?;
        let by_id = catalog
            .packs
            .iter()
            .cloned()
            .map(|p| (p.pack_id.clone(), p))
            .collect();
        Ok(Self { catalog, by_id })
    }
    pub fn catalog(&self) -> &PackCatalog {
        &self.catalog
    }
    pub fn discover_theme(&self, theme: &str) -> Vec<&CatalogPack> {
        self.catalog
            .packs
            .iter()
            .filter(|p| p.themes.iter().any(|x| x == theme))
            .collect()
    }
    pub fn activate(
        &self,
        requested: &[PackPin],
        allowlist: &BTreeSet<PackPin>,
    ) -> Result<Vec<&CatalogPack>, ActivationError> {
        let selected = requested
            .iter()
            .map(|p| (p.pack_id.as_str(), p))
            .collect::<BTreeMap<_, _>>();
        let mut active = Vec::new();
        for pin in requested {
            let pack = self
                .by_id
                .get(&pin.pack_id)
                .ok_or_else(|| ActivationError::Unknown(pin.pack_id.clone()))?;
            if pack.pack_version != pin.pack_version
                || pack.manifest_digest != pin.manifest_digest
                || !allowlist.contains(pin)
            {
                return Err(ActivationError::NotAllowlisted(pin.pack_id.clone()));
            }
            if pack.lifecycle != PackLifecycle::Approved
                || pack.trust_tier != PackTrustTier::CuratedEmbedded
            {
                return Err(ActivationError::NotApproved(pin.pack_id.clone()));
            }
            for dep in &pack.dependencies {
                if selected.get(dep.pack_id.as_str()) != Some(&dep) {
                    return Err(ActivationError::MissingDependency {
                        pack: pack.pack_id.clone(),
                        dependency: dep.pack_id.clone(),
                    });
                }
            }
            active.push(pack);
        }
        active.sort_by(|a, b| a.pack_id.cmp(&b.pack_id));
        Ok(active)
    }
}
pub fn catalog_registry() -> &'static CatalogRegistry {
    static REGISTRY: OnceLock<CatalogRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        CatalogRegistry::load(
            include_bytes!("../../data/packs/catalog-v1/catalog.json"),
            include_bytes!("../../data/packs/catalog-v1/manifest.json"),
        )
        .expect("embedded catalog is release-validated")
    })
}
pub fn embedded_activation_allowlist() -> BTreeSet<PackPin> {
    catalog_registry()
        .catalog()
        .packs
        .iter()
        .filter(|p| {
            p.lifecycle == PackLifecycle::Approved && p.trust_tier == PackTrustTier::CuratedEmbedded
        })
        .map(|p| PackPin {
            pack_id: p.pack_id.clone(),
            pack_version: p.pack_version,
            manifest_digest: p.manifest_digest.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pin(id: &str) -> PackPin {
        let p = catalog_registry()
            .catalog()
            .packs
            .iter()
            .find(|p| p.pack_id == id)
            .unwrap();
        PackPin {
            pack_id: p.pack_id.clone(),
            pack_version: p.pack_version,
            manifest_digest: p.manifest_digest.clone(),
        }
    }
    #[test]
    fn allowlisted_activation_is_digest_pinned_and_dependency_closed() {
        let r = catalog_registry();
        let allow = embedded_activation_allowlist();
        let core = pin("philosophy-core-v1");
        let deep = pin("agency-responsibility-v1");
        assert!(r.activate(&[core.clone(), deep.clone()], &allow).is_ok());
        assert!(matches!(
            r.activate(&[deep], &allow),
            Err(ActivationError::MissingDependency { .. })
        ));
        let mut bad = core;
        bad.manifest_digest = "0".repeat(64);
        assert!(matches!(
            r.activate(&[bad], &allow),
            Err(ActivationError::NotAllowlisted(_))
        ));
    }
    #[test]
    fn discovery_is_not_authority_and_candidates_are_rejected() {
        let r = catalog_registry();
        let p = pin("ethics-social-order-v1");
        assert_eq!(r.discover_theme("ethics-social-order").len(), 1);
        let mut allow = embedded_activation_allowlist();
        allow.insert(p.clone());
        assert!(matches!(
            r.activate(&[p], &allow),
            Err(ActivationError::NotApproved(_))
        ));
    }
}
