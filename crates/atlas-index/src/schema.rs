//! Types for the six YAMLs that sit at the Atlas/Ravel-Lite boundary:
//! `components.yaml`, `components.overrides.yaml`,
//! `external-components.yaml`, `subsystems.yaml`,
//! `subsystems.overrides.yaml`, and `related-components.yaml`.
//!
//! All but the last are owned by this crate. `related-components.yaml`
//! is owned by `component-ontology` and re-exported here so consumers
//! only need one crate. Each generated file carries its own
//! `schema_version` — independent versions let us evolve one file
//! without forcing a reader to relearn all six at once.
//!
//! `kind`, `role`, `language`, and `build_system` are kept as `String`
//! at this layer. The typed `ComponentKind` enum lands in `atlas-engine`
//! (see backlog task 5); anchoring the vocabulary to a not-yet-written
//! enum here would churn every downstream consumer every time the
//! vocabulary grows.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use component_ontology::{ComponentId, EvidenceGrade, LifecycleScope};
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const COMPONENTS_SCHEMA_VERSION: u32 = 1;
pub const OVERRIDES_SCHEMA_VERSION: u32 = 1;
pub const EXTERNALS_SCHEMA_VERSION: u32 = 1;
pub const SUBSYSTEMS_OVERRIDES_SCHEMA_VERSION: u32 = 1;
pub const SUBSYSTEMS_SCHEMA_VERSION: u32 = 1;

/// Fingerprint set written into `components.yaml` so a second tool run
/// can detect whether any cache-invalidating input changed. `prompt_shas`
/// is keyed by prompt id (e.g., `"classify"`, `"stage1-surface"`); all
/// SHAs are stored as lowercase hex strings to keep the YAML diffable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CacheFingerprints {
    pub ontology_sha: String,
    #[serde(default)]
    pub prompt_shas: BTreeMap<String, String>,
    pub model_id: String,
    pub backend_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathSegment {
    /// Relative to `ComponentsFile::root`.
    pub path: PathBuf,
    /// Hex-encoded SHA256 of the directory tree at `path`. Rename-match
    /// compares prior vs. new segments by this value (see
    /// `rename_match.rs`).
    pub content_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocAnchor {
    /// Relative to `ComponentsFile::root`.
    pub path: PathBuf,
    /// ATX heading text (no leading `#` marks).
    pub heading: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentEntry {
    pub id: ComponentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ComponentId>,
    pub kind: String,
    #[serde(default)]
    pub lifecycle_roles: Vec<LifecycleScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_system: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default)]
    pub path_segments: Vec<PathSegment>,
    #[serde(default)]
    pub manifests: Vec<PathBuf>,
    #[serde(default)]
    pub doc_anchors: Vec<DocAnchor>,
    pub evidence_grade: EvidenceGrade,
    #[serde(default)]
    pub evidence_fields: Vec<String>,
    pub rationale: String,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentsFile {
    pub schema_version: u32,
    pub root: PathBuf,
    pub generated_at: String,
    pub cache_fingerprints: CacheFingerprints,
    #[serde(default)]
    pub components: Vec<ComponentEntry>,
}

impl Default for ComponentsFile {
    fn default() -> Self {
        ComponentsFile {
            schema_version: COMPONENTS_SCHEMA_VERSION,
            root: PathBuf::new(),
            generated_at: String::new(),
            cache_fingerprints: CacheFingerprints::default(),
            components: Vec::new(),
        }
    }
}

/// Pin value for `components.overrides.yaml`.
///
/// Carries hand-written guidance for one slot in a component's pin map.
/// `Value` overrides a classification field (kind, role, language, …);
/// `Suppress` removes the component entirely; `SuppressChildren` prunes
/// specific children of the component.
///
/// On the wire, each variant accepts a natural form (a map for `Value`,
/// a bool for `Suppress`, a sequence for `SuppressChildren`) plus, for
/// `Suppress` and `SuppressChildren` only, a legacy doubly-nested map
/// form left over from when the enum used `#[serde(untagged)]` field-name
/// dispatch. Output (`Serialize`) is always the natural form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinValue {
    Suppress {
        suppress: AlwaysTrue,
    },
    SuppressChildren {
        suppress_children: Vec<ComponentId>,
    },
    Value {
        value: String,
        reason: Option<String>,
    },
}

/// Marker that serialises as the literal `true` and rejects anything
/// else on deserialisation. Required because the `Suppress` variant
/// above must accept only `suppress: true` — silently accepting
/// `suppress: false` would let users disable a pin by typo.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AlwaysTrue;

impl Serialize for AlwaysTrue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        true.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AlwaysTrue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = bool::deserialize(deserializer)?;
        if !v {
            return Err(serde::de::Error::custom(
                "`suppress: false` is not meaningful; remove the pin instead",
            ));
        }
        Ok(AlwaysTrue)
    }
}

impl Serialize for PinValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            PinValue::Suppress { .. } => serializer.serialize_bool(true),
            PinValue::SuppressChildren { suppress_children } => {
                suppress_children.serialize(serializer)
            }
            PinValue::Value { value, reason } => {
                let len = if reason.is_some() { 2 } else { 1 };
                let mut map = serializer.serialize_map(Some(len))?;
                map.serialize_entry("value", value)?;
                if let Some(r) = reason {
                    map.serialize_entry("reason", r)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for PinValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(PinValueVisitor)
    }
}

struct PinValueVisitor;

impl<'de> Visitor<'de> for PinValueVisitor {
    type Value = PinValue;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(
            "a pin value: `true` to suppress the component, a sequence of child ids \
             to suppress specific children, or a `{value, reason?}` map to override a field",
        )
    }

    fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
        if !v {
            return Err(E::custom(
                "`suppress: false` is not meaningful; remove the pin instead",
            ));
        }
        Ok(PinValue::Suppress {
            suppress: AlwaysTrue,
        })
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut items: Vec<ComponentId> = Vec::new();
        while let Some(item) = seq.next_element::<ComponentId>()? {
            items.push(item);
        }
        Ok(PinValue::SuppressChildren {
            suppress_children: items,
        })
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let first_key: Option<String> = map.next_key()?;
        match first_key.as_deref() {
            Some("value") => {
                let value: String = map.next_value()?;
                let mut reason: Option<String> = None;
                while let Some(k) = map.next_key::<String>()? {
                    match k.as_str() {
                        "reason" => reason = Some(map.next_value()?),
                        other => {
                            return Err(serde::de::Error::unknown_field(
                                other,
                                &["value", "reason"],
                            ));
                        }
                    }
                }
                Ok(PinValue::Value { value, reason })
            }
            Some("reason") => {
                // Field order is not guaranteed when YAML is round-tripped through
                // serde data models; accept {reason, value} too.
                let reason: String = map.next_value()?;
                let mut value: Option<String> = None;
                while let Some(k) = map.next_key::<String>()? {
                    match k.as_str() {
                        "value" => value = Some(map.next_value()?),
                        other => {
                            return Err(serde::de::Error::unknown_field(
                                other,
                                &["value", "reason"],
                            ));
                        }
                    }
                }
                let value = value.ok_or_else(|| serde::de::Error::missing_field("value"))?;
                Ok(PinValue::Value {
                    value,
                    reason: Some(reason),
                })
            }
            // Legacy doubly-nested form: `suppress: { suppress: true }` was the only
            // accepted shape before this enum gained custom (de)serialise impls. Still
            // accepted on input because pre-existing override files in the wild may use
            // it; output always emits the natural single-nested form.
            Some("suppress") => {
                let v: bool = map.next_value()?;
                if !v {
                    return Err(serde::de::Error::custom(
                        "`suppress: false` is not meaningful; remove the pin instead",
                    ));
                }
                if map.next_key::<String>()?.is_some() {
                    return Err(serde::de::Error::custom(
                        "`suppress` pin accepts only a single `suppress: true` field",
                    ));
                }
                Ok(PinValue::Suppress {
                    suppress: AlwaysTrue,
                })
            }
            Some("suppress_children") => {
                let children: Vec<ComponentId> = map.next_value()?;
                if map.next_key::<String>()?.is_some() {
                    return Err(serde::de::Error::custom(
                        "`suppress_children` pin accepts only a single `suppress_children: [...]` field",
                    ));
                }
                Ok(PinValue::SuppressChildren {
                    suppress_children: children,
                })
            }
            None => Err(serde::de::Error::custom("empty pin value")),
            Some(other) => Err(serde::de::Error::custom(format!(
                "unknown pin value field `{other}`; expected `value` (with optional `reason`), \
                 a bool, or a sequence of child ids"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverridesFile {
    pub schema_version: u32,
    /// Outer key: component id. Inner key: field name being pinned
    /// (e.g., `"role"`, `"kind"`, `"deleted"`). `BTreeMap` keeps the
    /// serialised output deterministic.
    #[serde(default)]
    pub pins: BTreeMap<ComponentId, BTreeMap<String, PinValue>>,
    /// Components authored by hand when no evidence exists for them
    /// (e.g., a spec directory with no manifest). They bypass L2/L3 and
    /// land in `components.yaml` directly.
    #[serde(default)]
    pub additions: Vec<ComponentEntry>,
}

impl Default for OverridesFile {
    fn default() -> Self {
        OverridesFile {
            schema_version: OVERRIDES_SCHEMA_VERSION,
            pins: BTreeMap::new(),
            additions: Vec::new(),
        }
    }
}

/// Hand-authored subsystem boundary. Lives in `subsystems.overrides.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubsystemOverride {
    pub id: String,
    /// Mixed glob and id forms. A `members` entry containing `/` or `*`
    /// is matched against component path segments; otherwise it is
    /// matched against component id directly. See the design spec for
    /// the resolution algorithm.
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default)]
    pub lifecycle_roles: Vec<LifecycleScope>,
    pub rationale: String,
    pub evidence_grade: EvidenceGrade,
    #[serde(default)]
    pub evidence_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubsystemsOverridesFile {
    pub schema_version: u32,
    #[serde(default)]
    pub subsystems: Vec<SubsystemOverride>,
}

impl Default for SubsystemsOverridesFile {
    fn default() -> Self {
        SubsystemsOverridesFile {
            schema_version: SUBSYSTEMS_OVERRIDES_SCHEMA_VERSION,
            subsystems: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberEvidence {
    /// The source member entry's role here is dual: when `matched_via`
    /// indicates a successful resolution (`"id"` or a glob string),
    /// `id` carries the resolved component id. When `matched_via`
    /// indicates a resolution failure (`"<glob> (no matches)"`,
    /// `"invalid glob"`, `"no such component"`, etc.), `id` carries
    /// the raw source member string verbatim. Kept as `String` so a
    /// failed resolution can still be recorded faithfully — this field
    /// is an audit log entry, not a join key.
    pub id: String,
    /// Glob string when the member resolved via a glob, the literal
    /// `"id"` when the member entry was an id form, or
    /// `"<glob> (no matches)"` when a glob produced zero matches.
    pub matched_via: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubsystemEntry {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default)]
    pub lifecycle_roles: Vec<LifecycleScope>,
    pub rationale: String,
    pub evidence_grade: EvidenceGrade,
    #[serde(default)]
    pub evidence_fields: Vec<String>,
    /// Resolved component ids, sorted and deduped.
    #[serde(default)]
    pub members: Vec<ComponentId>,
    /// Audit trail: how each resolved member was matched.
    #[serde(default)]
    pub member_evidence: Vec<MemberEvidence>,
    /// Soft warnings about this subsystem (e.g. `"all members unresolved"`).
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubsystemsFile {
    pub schema_version: u32,
    pub generated_at: String,
    #[serde(default)]
    pub subsystems: Vec<SubsystemEntry>,
}

impl Default for SubsystemsFile {
    fn default() -> Self {
        SubsystemsFile {
            schema_version: SUBSYSTEMS_SCHEMA_VERSION,
            generated_at: String::new(),
            subsystems: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalEntry {
    pub id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Package URL (pURL) spec identifier, when a manifest supplies one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purl: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Manifest paths (relative to the repo root) that surfaced this
    /// external. Multiple entries are expected — the same crate can
    /// show up in several `Cargo.toml`s.
    #[serde(default)]
    pub discovered_from: Vec<String>,
    pub evidence_grade: EvidenceGrade,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalsFile {
    pub schema_version: u32,
    #[serde(default)]
    pub externals: Vec<ExternalEntry>,
}

impl Default for ExternalsFile {
    fn default() -> Self {
        ExternalsFile {
            schema_version: EXTERNALS_SCHEMA_VERSION,
            externals: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_value_value_variant_round_trips() {
        let pin = PinValue::Value {
            value: "library".into(),
            reason: Some("override the classifier".into()),
        };
        let yaml = serde_yaml::to_string(&pin).unwrap();
        let parsed: PinValue = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, pin);
    }

    #[test]
    fn pin_value_value_variant_omits_none_reason_in_yaml() {
        let pin = PinValue::Value {
            value: "library".into(),
            reason: None,
        };
        let yaml = serde_yaml::to_string(&pin).unwrap();
        assert!(
            !yaml.contains("reason"),
            "reason: null should be skipped, got:\n{yaml}"
        );
        let parsed: PinValue = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, pin);
    }

    #[test]
    fn pin_value_suppress_serialises_as_bare_true() {
        let pin = PinValue::Suppress {
            suppress: AlwaysTrue,
        };
        let yaml = serde_yaml::to_string(&pin).unwrap();
        assert_eq!(yaml.trim(), "true", "got:\n{yaml}");
        let parsed: PinValue = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, pin);
    }

    #[test]
    fn pin_value_suppress_accepts_legacy_doubly_nested_form() {
        let pin: PinValue = serde_yaml::from_str("suppress: true").unwrap();
        assert_eq!(
            pin,
            PinValue::Suppress {
                suppress: AlwaysTrue
            }
        );
    }

    #[test]
    fn pin_value_suppress_children_serialises_as_bare_sequence() {
        let pin = PinValue::SuppressChildren {
            suppress_children: vec![
                ComponentId::parse("a").unwrap(),
                ComponentId::parse("b").unwrap(),
            ],
        };
        let yaml = serde_yaml::to_string(&pin).unwrap();
        // Natural form: a YAML sequence at the root, no `suppress_children:` wrapper.
        assert!(!yaml.contains("suppress_children"), "got:\n{yaml}");
        assert!(yaml.contains("- a"), "got:\n{yaml}");
        assert!(yaml.contains("- b"), "got:\n{yaml}");
        let parsed: PinValue = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, pin);
    }

    #[test]
    fn pin_value_suppress_children_accepts_natural_inline_sequence() {
        // Reported user form: `suppress_children: [a, b]` written as the inner-map
        // value. The PinValue deserialiser sees only the value side (`[a, b]`).
        let pin: PinValue = serde_yaml::from_str("[a, b]").unwrap();
        assert_eq!(
            pin,
            PinValue::SuppressChildren {
                suppress_children: vec![
                    ComponentId::parse("a").unwrap(),
                    ComponentId::parse("b").unwrap(),
                ]
            }
        );
    }

    #[test]
    fn pin_value_suppress_children_accepts_legacy_doubly_nested_form() {
        let pin: PinValue = serde_yaml::from_str("suppress_children:\n- a\n- b\n").unwrap();
        assert_eq!(
            pin,
            PinValue::SuppressChildren {
                suppress_children: vec![
                    ComponentId::parse("a").unwrap(),
                    ComponentId::parse("b").unwrap(),
                ]
            }
        );
    }

    #[test]
    fn pin_value_suppress_rejects_bare_false() {
        let err = serde_yaml::from_str::<PinValue>("false").unwrap_err();
        assert!(
            err.to_string().contains("not meaningful"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn pin_value_suppress_rejects_legacy_false() {
        let err = serde_yaml::from_str::<PinValue>("suppress: false").unwrap_err();
        assert!(
            err.to_string().contains("not meaningful"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn pin_value_value_variant_with_only_value_serialises_compactly() {
        let pin = PinValue::Value {
            value: "rust-library".into(),
            reason: None,
        };
        let yaml = serde_yaml::to_string(&pin).unwrap();
        assert!(yaml.contains("value: rust-library"), "got:\n{yaml}");
        assert!(!yaml.contains("reason"), "got:\n{yaml}");
    }

    #[test]
    fn pin_value_value_variant_unknown_field_is_rejected() {
        // A typo like `valeu:` lands us in the "unknown field" branch rather
        // than silently degrading to a different variant.
        let err = serde_yaml::from_str::<PinValue>("valeu: rust-library").unwrap_err();
        assert!(
            err.to_string().contains("unknown pin value field"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn components_file_default_has_current_schema_version() {
        let f = ComponentsFile::default();
        assert_eq!(f.schema_version, COMPONENTS_SCHEMA_VERSION);
        assert!(f.components.is_empty());
    }

    #[test]
    fn overrides_file_default_has_current_schema_version() {
        let f = OverridesFile::default();
        assert_eq!(f.schema_version, OVERRIDES_SCHEMA_VERSION);
    }

    #[test]
    fn externals_file_default_has_current_schema_version() {
        let f = ExternalsFile::default();
        assert_eq!(f.schema_version, EXTERNALS_SCHEMA_VERSION);
    }

    #[test]
    fn component_entry_round_trips_through_yaml() {
        let entry = ComponentEntry {
            id: ComponentId::parse("workspace/crate-a").unwrap(),
            parent: Some(ComponentId::parse("workspace").unwrap()),
            kind: "rust-library".into(),
            lifecycle_roles: vec![LifecycleScope::Build, LifecycleScope::Runtime],
            language: Some("rust".into()),
            build_system: Some("cargo".into()),
            role: Some("library".into()),
            path_segments: vec![PathSegment {
                path: PathBuf::from("crates/crate-a"),
                content_sha: "abc123".into(),
            }],
            manifests: vec![PathBuf::from("crates/crate-a/Cargo.toml")],
            doc_anchors: vec![DocAnchor {
                path: PathBuf::from("crates/crate-a/README.md"),
                heading: "Crate A".into(),
            }],
            evidence_grade: EvidenceGrade::Strong,
            evidence_fields: vec!["Cargo.toml:[lib]".into()],
            rationale: "has Cargo.toml [lib] section".into(),
            deleted: false,
        };
        let yaml = serde_yaml::to_string(&entry).unwrap();
        let parsed: ComponentEntry = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, entry);
    }

    #[test]
    fn subsystems_overrides_file_default_has_current_schema_version() {
        let f = SubsystemsOverridesFile::default();
        assert_eq!(f.schema_version, SUBSYSTEMS_OVERRIDES_SCHEMA_VERSION);
        assert!(f.subsystems.is_empty());
    }

    #[test]
    fn subsystem_override_round_trips_through_yaml() {
        let s = SubsystemOverride {
            id: "auth".into(),
            members: vec!["services/auth/*".into(), "identity-core".into()],
            role: Some("identity-and-authorisation".into()),
            lifecycle_roles: vec![LifecycleScope::Runtime],
            rationale: "owns all session/token surfaces".into(),
            evidence_grade: EvidenceGrade::Strong,
            evidence_fields: vec![],
        };
        let yaml = serde_yaml::to_string(&s).unwrap();
        let parsed: SubsystemOverride = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, s);
    }

    #[test]
    fn subsystems_overrides_file_round_trips_through_yaml() {
        let f = SubsystemsOverridesFile {
            schema_version: SUBSYSTEMS_OVERRIDES_SCHEMA_VERSION,
            subsystems: vec![SubsystemOverride {
                id: "auth".into(),
                members: vec!["libs/identity".into()],
                role: None,
                lifecycle_roles: vec![],
                rationale: "x".into(),
                evidence_grade: EvidenceGrade::Strong,
                evidence_fields: vec![],
            }],
        };
        let yaml = serde_yaml::to_string(&f).unwrap();
        let parsed: SubsystemsOverridesFile = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, f);
    }

    #[test]
    fn subsystems_file_default_has_current_schema_version() {
        let f = SubsystemsFile::default();
        assert_eq!(f.schema_version, SUBSYSTEMS_SCHEMA_VERSION);
        assert!(f.subsystems.is_empty());
        assert!(f.generated_at.is_empty());
    }

    #[test]
    fn subsystem_entry_round_trips_through_yaml() {
        let e = SubsystemEntry {
            id: "auth".into(),
            role: Some("identity".into()),
            lifecycle_roles: vec![LifecycleScope::Runtime],
            rationale: "x".into(),
            evidence_grade: EvidenceGrade::Strong,
            evidence_fields: vec![],
            members: vec![
                ComponentId::parse("auth-service").unwrap(),
                ComponentId::parse("identity-lib").unwrap(),
            ],
            member_evidence: vec![
                MemberEvidence {
                    id: "auth-service".into(),
                    matched_via: "services/auth/*".into(),
                },
                MemberEvidence {
                    id: "identity-lib".into(),
                    matched_via: "libs/identity".into(),
                },
            ],
            notes: vec![],
        };
        let yaml = serde_yaml::to_string(&e).unwrap();
        let parsed: SubsystemEntry = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, e);
    }

    #[test]
    fn member_evidence_round_trips_through_yaml() {
        let m = MemberEvidence {
            id: "x-component".into(),
            matched_via: "id".into(),
        };
        let yaml = serde_yaml::to_string(&m).unwrap();
        let parsed: MemberEvidence = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, m);
    }

    #[test]
    fn external_entry_round_trips_through_yaml() {
        let entry = ExternalEntry {
            id: "crate:serde".into(),
            kind: "external".into(),
            language: Some("rust".into()),
            purl: Some("pkg:cargo/serde@1".into()),
            homepage: None,
            url: None,
            discovered_from: vec!["Cargo.toml".into()],
            evidence_grade: EvidenceGrade::Strong,
        };
        let yaml = serde_yaml::to_string(&entry).unwrap();
        let parsed: ExternalEntry = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, entry);
    }
}
