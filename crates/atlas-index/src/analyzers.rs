//! `<output>/.atlas/analyzers.yaml` schema (Atlas vNext Phase 1).
//!
//! Declarative analyser registry overrides. The shipped registry has
//! built-in defaults (Cargo classifier, Dockerfile classifier,
//! LLM-classify fallback in Phase 1; more in Phase 2). The
//! `analyzers.yaml` file extends or overrides them per workspace
//! (design §5.2, §6.6, §7).
//!
//! This module is types-only; the dispatcher and registry live in
//! `crates/atlas-analyzers` (PR-5). Phase 1 ships
//! `schema_version: 1`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Schema version for `analyzers.yaml`.
pub const ANALYZERS_SCHEMA_VERSION: u32 = 1;

/// Which L-stage an analyser plugs into. Closed kebab-case enum so a
/// typo in a YAML registry file fails loudly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    L1,
    L2,
    L3,
    L4,
    L5,
    L6,
    L7,
    L8,
    L9,
}

/// Cost class of an analyser. The dispatcher picks the cheapest
/// applicable analyser whose confidence reaches the configured
/// threshold (design §7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CostClass {
    /// In-process pure-Rust parser; microseconds.
    DeterministicCheap,
    /// Subprocess call to a language-specific tool (e.g. rust-analyzer);
    /// seconds.
    DeterministicExpensive,
    /// LLM call against a small / fast model (haiku-class).
    LlmCheap,
    /// LLM call against a large / slow model (sonnet-/opus-class).
    LlmExpensive,
}

/// Confidence shape an analyser declares. `Binary` analysers always
/// produce `Confident` or `Error`; `Graded` analysers attach a
/// `[0, 1]` confidence; `Declines` analysers are fallthrough-only
/// (e.g. the LLM classify analyser declines anything a deterministic
/// analyser already handled).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Confidence {
    Binary,
    Graded,
    Declines,
}

/// Process-boundary transport for the analyser. Phase 1 ships only
/// `in-process`; Phase 2 adds `subprocess` for crash isolation
/// (design §5.2, §7.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "kebab-case")]
pub enum Transport {
    /// Rust trait object inside the engine process.
    InProcess,
    /// Spawned subprocess speaking line-delimited JSON over stdio
    /// (design §7.2).
    Subprocess {
        /// argv of the subprocess (analyser binary + args). Resolved
        /// against `$PATH` and the workspace `override_search`.
        command: Vec<String>,
        /// Hard timeout per request.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_seconds: Option<u32>,
    },
}

/// Applicability predicate for an analyser. Phase 1 supports four
/// shapes (design §6.6 example): file globs, language tags, manifest
/// types, and unconditional `always`. Modelled as a struct of
/// optional fields so the YAML reads naturally — combining a
/// `file_globs` filter with a `languages` filter is the conjunction.
///
/// All four fields are optional; an analyser whose predicate has
/// `always: true` (and nothing else set) is the LLM-fallback shape.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ApplicabilityPredicate {
    /// glob patterns matched against file paths relative to the
    /// component root.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_globs: Vec<String>,
    /// Language tags (`rust`, `typescript`, …). Matches when the
    /// target's language set intersects this list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub languages: Vec<String>,
    /// Manifest-type tags (`cargo`, `npm`, `helm`, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub manifest_types: Vec<String>,
    /// Unconditional applicability. Used by the LLM-fallback analyser
    /// in combination with `confidence: declines` so it only runs
    /// when no other analyser was confident.
    #[serde(default, skip_serializing_if = "is_false")]
    pub always: bool,
}

/// Skip-serialise helper for the `always` boolean — keeps the
/// emitted YAML small for the common case (`always: false`, written
/// as the field's absence).
fn is_false(b: &bool) -> bool {
    !*b
}

/// One analyser declaration. Mirrors the design §6.6 example shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyzerSpec {
    /// Stable analyser id (`cargo-toml-classifier`, `dockerfile-l1`,
    /// …). Contributes to fingerprint inputs (design §8.1).
    pub id: String,
    pub stage: Stage,
    pub applicability: ApplicabilityPredicate,
    pub cost_class: CostClass,
    /// Optional. Defaults to `Binary` for in-process deterministic
    /// analysers; emit explicitly for LLM analysers and any analyser
    /// whose semantics include `declines`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
    /// Process boundary (in-process or subprocess). Flattened into
    /// the spec so the YAML reads naturally:
    /// `transport: in-process` or `transport: subprocess` plus
    /// `command:` / `timeout_seconds:`.
    #[serde(flatten)]
    pub transport: Transport,
    /// Analyser version string. Free-form; the dispatcher includes
    /// this in cache fingerprints so a version bump invalidates
    /// cached outputs.
    pub version: String,
}

/// Top-level shape of `<output>/.atlas/analyzers.yaml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalyzersFile {
    pub schema_version: u32,
    #[serde(default)]
    pub analyzers: Vec<AnalyzerSpec>,
    /// Optional per-stage configuration the registry passes through
    /// to analysers. Open-ended so the file can carry tuning knobs
    /// without churning the schema. Keyed by analyser id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub config: BTreeMap<String, serde_yaml::Value>,
}

impl Default for AnalyzersFile {
    fn default() -> Self {
        AnalyzersFile {
            schema_version: ANALYZERS_SCHEMA_VERSION,
            analyzers: Vec::new(),
            config: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_in_process_spec() -> AnalyzerSpec {
        AnalyzerSpec {
            id: "cargo-toml-classifier".into(),
            stage: Stage::L3,
            applicability: ApplicabilityPredicate {
                file_globs: vec!["**/Cargo.toml".into()],
                ..Default::default()
            },
            cost_class: CostClass::DeterministicCheap,
            confidence: None,
            transport: Transport::InProcess,
            version: "1.0.3".into(),
        }
    }

    fn sample_subprocess_spec() -> AnalyzerSpec {
        AnalyzerSpec {
            id: "rust-analyzer-surface".into(),
            stage: Stage::L5,
            applicability: ApplicabilityPredicate {
                languages: vec!["rust".into()],
                ..Default::default()
            },
            cost_class: CostClass::DeterministicExpensive,
            confidence: Some(Confidence::Binary),
            transport: Transport::Subprocess {
                command: vec!["rust-analyzer-surface".into(), "--stage=L5".into()],
                timeout_seconds: Some(60),
            },
            version: "0.4.1".into(),
        }
    }

    fn sample_llm_fallback_spec() -> AnalyzerSpec {
        AnalyzerSpec {
            id: "llm-classify-fallback".into(),
            stage: Stage::L3,
            applicability: ApplicabilityPredicate {
                always: true,
                ..Default::default()
            },
            cost_class: CostClass::LlmCheap,
            confidence: Some(Confidence::Declines),
            transport: Transport::InProcess,
            version: "1.0.0".into(),
        }
    }

    fn sample_analyzers_file() -> AnalyzersFile {
        AnalyzersFile {
            schema_version: ANALYZERS_SCHEMA_VERSION,
            analyzers: vec![
                sample_in_process_spec(),
                sample_subprocess_spec(),
                sample_llm_fallback_spec(),
            ],
            config: BTreeMap::new(),
        }
    }

    #[test]
    fn analyzer_spec_round_trips_through_yaml() {
        for original in [
            sample_in_process_spec(),
            sample_subprocess_spec(),
            sample_llm_fallback_spec(),
        ] {
            let yaml = serde_yaml::to_string(&original).unwrap();
            let parsed: AnalyzerSpec = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(parsed, original);
        }
    }

    #[test]
    fn analyzers_file_round_trips_through_yaml() {
        let original = sample_analyzers_file();
        let yaml = serde_yaml::to_string(&original).unwrap();
        let parsed: AnalyzersFile = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn stage_variants_serialise_kebab_case() {
        for (stage, expected) in [
            (Stage::L1, "l1"),
            (Stage::L2, "l2"),
            (Stage::L3, "l3"),
            (Stage::L4, "l4"),
            (Stage::L5, "l5"),
            (Stage::L6, "l6"),
            (Stage::L7, "l7"),
            (Stage::L8, "l8"),
            (Stage::L9, "l9"),
        ] {
            let yaml = serde_yaml::to_string(&stage).unwrap();
            assert_eq!(yaml.trim(), expected);
            let parsed: Stage = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(parsed, stage);
        }
    }

    #[test]
    fn cost_class_variants_serialise_kebab_case() {
        for (cls, expected) in [
            (CostClass::DeterministicCheap, "deterministic-cheap"),
            (CostClass::DeterministicExpensive, "deterministic-expensive"),
            (CostClass::LlmCheap, "llm-cheap"),
            (CostClass::LlmExpensive, "llm-expensive"),
        ] {
            let yaml = serde_yaml::to_string(&cls).unwrap();
            assert_eq!(yaml.trim(), expected);
            let parsed: CostClass = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(parsed, cls);
        }
    }

    #[test]
    fn confidence_variants_serialise_kebab_case() {
        for (c, expected) in [
            (Confidence::Binary, "binary"),
            (Confidence::Graded, "graded"),
            (Confidence::Declines, "declines"),
        ] {
            let yaml = serde_yaml::to_string(&c).unwrap();
            assert_eq!(yaml.trim(), expected);
            let parsed: Confidence = serde_yaml::from_str(&yaml).unwrap();
            assert_eq!(parsed, c);
        }
    }

    #[test]
    fn transport_in_process_serialises_with_kebab_case_tag() {
        let yaml = serde_yaml::to_string(&Transport::InProcess).unwrap();
        assert!(yaml.contains("transport: in-process"), "got:\n{yaml}");
        let parsed: Transport = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, Transport::InProcess);
    }

    #[test]
    fn transport_subprocess_round_trips() {
        let original = Transport::Subprocess {
            command: vec!["bin".into(), "--flag".into()],
            timeout_seconds: Some(30),
        };
        let yaml = serde_yaml::to_string(&original).unwrap();
        assert!(yaml.contains("transport: subprocess"), "got:\n{yaml}");
        let parsed: Transport = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn applicability_predicate_round_trips_with_only_some_fields() {
        // Only file_globs set — other fields skipped on serialise.
        let original = ApplicabilityPredicate {
            file_globs: vec!["**/Dockerfile".into()],
            ..Default::default()
        };
        let yaml = serde_yaml::to_string(&original).unwrap();
        assert!(!yaml.contains("languages"), "got:\n{yaml}");
        assert!(!yaml.contains("manifest_types"), "got:\n{yaml}");
        assert!(!yaml.contains("always"), "got:\n{yaml}");
        let parsed: ApplicabilityPredicate = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn applicability_predicate_always_round_trips() {
        let original = ApplicabilityPredicate {
            always: true,
            ..Default::default()
        };
        let yaml = serde_yaml::to_string(&original).unwrap();
        assert!(yaml.contains("always: true"), "got:\n{yaml}");
        let parsed: ApplicabilityPredicate = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn analyzers_file_default_has_current_schema_version() {
        let f = AnalyzersFile::default();
        assert_eq!(f.schema_version, ANALYZERS_SCHEMA_VERSION);
        assert!(f.analyzers.is_empty());
        assert!(f.config.is_empty());
    }
}
