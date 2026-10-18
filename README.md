# Arnes

Configuration, diagnostics, synchronization and export for Claude, Codex and Cursor.
Prepared locally for `@sebastienelet/arnes@1.0.0`; no registry release is qualified yet.

| Capability | Owner / public command |
| --- | --- |
| Configuration and diagnosis | `@sebastienelet/arnes`: `arnes doctor` |
| Synchronization and hook declarations | `@sebastienelet/arnes`: `arnes sync`, `arnes setup hooks` |
| Configuration export | `@sebastienelet/arnes`: `arnes export` |
| Capture, outcomes, feedback and reports | `@sebastienelet/agent-measure`: `agent-measure` |
| Evaluation, validation and comparison | `@sebastienelet/agent-experiments`: `agent-experiments` |
| Instruction injection | `@sebastienelet/agent-skills`: `output-discipline-hook`, prepared by its owner |

Use Bun 1.4.3 to acquire the exact published version once available. macOS ARM64 and
Linux x64 glibc are the intended native targets; other targets are rejected. Installation
only acquires files. Hook activation is explicit through `arnes setup hooks`.

Set `ARNES_MANIFEST` to an absolute YAML manifest and `ARNES_REPOSITORY_ROOT` to an
existing absolute configuration/resource directory. It defaults to the working directory.
`ARNES_SOURCE_ROOT` can separately select the absolute user instruction/skill deployment
root; it defaults to the repository root. Set both roots for a wholly independent source
bundle. Arnes never infers a checkout from the manifest's symlink. The manifest defaults to
`$HOME/.arnes.yaml`. See `examples/manifest.yaml` for an empty configuration and `--help`
for resource commands. Existing manifest schema 1 is retained.

`arnes export --sources export.json` exports an explicit JSON object mapping relative
file paths to `instructions`, `commands-rules`, `skills`, `hooks-routing` or `services`.
`--check` detects drift. Sources must remain inside the supplied root; ignored, sensitive,
non-regular and hardlinked sources are refused. Name-based secret detection is limited.
Exports default to `.harness-export`. The old harness layout remains available when
`--sources` is omitted, for compatibility with existing exports.

Measurement hooks invoke `~/.local/bin/agent-measure hook --agent …`. The optional
output-discipline hook invokes `~/.local/bin/output-discipline-hook`, owned with its
skill by agent-skills. Arnes does not ship that injector. Acquisition and activation of
these companions remain separate. Missing required executables prevent hook setup;
foreign handlers are preserved. Doctor reports observed configuration, not activation.

Contribute with Rust 1.98.1, Bun 1.4.3 and Git: `bun install --frozen-lockfile`,
`cargo test --locked`, `bun run build`, `bun test tests/launcher.test.mjs`.
Native packages carry executable SHA-256 and source revision in `BUILD.json`.
The manual publish workflow requires configured npm trusted publishers (OIDC) for all
three packages and runs `npm publish --provenance` only in CI. Publish native variants
before the entry package. Inspect registry state before resuming a partial publication.
