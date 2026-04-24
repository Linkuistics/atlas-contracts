# atlas-contracts

Public data-format, vocabulary, and schema crates for [Atlas](https://github.com/linkuistics/Atlas). These are the contracts Atlas produces and that downstream tools consume — extracted into a public repository so that OSS consumers (notably [Ravel-Lite](https://github.com/linkuistics/Ravel-Lite)) can depend on them without pulling in Atlas's private implementation.

## Crates

- **`component-ontology`** — host-agnostic ontology of component-relationship edges (kinds, lifecycles, evidence grades) and the `related-components.yaml` schema.
- **`atlas-index`** — schema and reader for the four Atlas YAMLs; rename-matching and merge-with-overrides.

## Consumption

These crates are intended to be consumed as git dependencies:

```toml
[dependencies]
component-ontology = { git = "https://github.com/linkuistics/atlas-contracts", rev = "<commit-sha>" }
atlas-index = { git = "https://github.com/linkuistics/atlas-contracts", rev = "<commit-sha>" }
```

Pin by commit sha for reproducible builds.

## What lives elsewhere

The computational parts of Atlas — the Salsa pipeline (`atlas-engine`), the CLI (`atlas-cli`), prompt templates, LLM wiring — remain in the private Atlas repository. This repo holds only the public vocabulary and file-format contracts.

## License

Apache-2.0 — see [LICENSE](LICENSE).
