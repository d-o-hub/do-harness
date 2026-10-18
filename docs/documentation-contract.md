# Documentation contract

Which documentation in this repository is generated, which is verified, what a
green check actually proves, and how to repair a failure. The executor is
[scripts/docs.py](../scripts/docs.py), the classification policy is
[contracts.toml](contracts.toml), and the generator is
`crates/do-harness/src/docs_generation` (compiled for tests only).

## Generated regions

The CLI definitions are the single source of truth for the mechanical facts a
reference document repeats: command paths, aliases, usage lines, argument
arity, required status, defaults, possible values, and help text. Three
artifacts are rendered from `Cli::command()`:

- [README.md](../README.md) — the `cli-commands` region.
- [cli.md](cli.md) — the `cli-globals` region plus one `cli-contract <path>`
  region per visible command path.
- `tests/fixtures/cli-contract.json` — the whole file, in the wire schema the
  example checker consumes; it carries no markers.

A region is delimited by two exact lines: `<!-- BEGIN GENERATED: <id> -->` and
`<!-- END GENERATED: <id> -->`. The parser rejects a BEGIN inside another
region, an END without a BEGIN, mismatched ids, and a duplicated id. The check
compares each region with the freshly rendered content byte for byte after
`\r\n` normalization, so a Windows checkout does not fail for its line endings;
rewriting preserves the file's own line-ending style and every byte outside the
region.

A failure names the file and region. A content diff is the only repairable
class: a missing file, unparsable markers, a region the CLI no longer produces,
and a region the CLI does produce but that is absent from the document are
structural, and the tool never inserts or deletes a marker pair on its own.

## Example blocks

A fenced CLI or config example in a maintained document must carry an adjacent
marker: `<!-- example: id=<id> classification=<class> [reason="..."] [fixture=<id>] -->`.

- Classifications are `executable`, `parser-check-only`, `illustrative`, and
  `historical`; every non-executable block must state a `reason`.
- An `executable` block names a fixture in
  `tests/fixtures/docs-examples/manifest.toml`. The fence text must equal that
  fixture's `displayed_command` (whitespace-normalized), and the fixture's
  commands then run the real `do-harness` binary in an isolated temporary
  directory, compared against the declared `expected_exit`; `file_assertions`
  check whatever the commands produced.
- A fixture may declare `platforms = ["linux", "darwin", "windows"]` for a
  contract it cannot satisfy on every host — the local installer fixture runs
  `scripts/test-install.sh`, which supports Linux and macOS only. A host outside
  the declared list prints the skip and does not run the fixture; an unknown
  platform name is a manifest error, never a silent exclusion.
- A `parser-check-only` block must parse as TOML or JSON.
- `illustrative` and `historical` blocks are inert: they are neither executed
  nor parsed, and only their `reason` is required.
- Fences are classified by language and content — bash-like languages, TOML
  containing `language =`, `[[sensors]]`, or `[signal-sets]`, JSON containing
  `"ok":`, `"schema_version":`, or `"sensors":`, and any fence whose body names
  `do-harness`. A generated region is exempt; any other such fence without a
  marker is a failure, and discovering zero blocks fails too.

## Scope

`contracts.toml` classifies every document with a path or glob rule that also
carries a classification (`maintained`, `historical`, `fixture`, `vendored`,
`canonical_alias`) and a non-empty reason. The most specific matching rule
wins; two rules of equal specificity that disagree are a hard error, and a
`canonical_alias` rule must name its `canonical_target`.

Discovery reads markdown through git (`ls-files --cached --others
--exclude-standard` with case-insensitive `*.md`, falling back to a filesystem
walk) and keeps every README-style file, every file under `docs/`, and every
path classified `maintained` or `canonical_alias`. A discovered document with
no rule, an empty maintained inventory, and zero discovered documents are each
a failure, so the gate cannot pass by checking nothing.

Three sensors gate the contract, all at `error` severity and all present in
both the `verification` and `release` signal sets. `docs-sync` runs
`python3 scripts/docs.py sync --check`, `docs-examples` runs
`python3 scripts/docs.py examples --check`, and `docs-links` runs
`python3 scripts/docs.py links --check`; each carries a `fix` hint that the
harness prints with a failing verdict.

## Impact analysis and review packets

`python3 scripts/docs.py impact --base REV --head REV --format json` emits a
versioned mechanical report and a separate reviewer packet. `[[impact_map]]`
entries in `contracts.toml` map relevant source paths to maintained Markdown
heading IDs and give each mapping a reason. The analyzer resolves the merge-base
and loads the policy from that revision; a policy edit in the head cannot erase
the obligations established by the base policy.

The report records resolved revisions and tree identity, policy/checker/input
digests, changed paths and source reasons, mechanical findings, generated-region
equality, verified example IDs, affected-section metadata and rationale
obligations, unknown scope, and local artifact references/digests.
It makes no LLM or network calls. No external CI artifact is accepted as
evidence. In particular, `verify.yml`'s `docs-sync-diff` upload is failure-only
diagnostic text whose payload has no machine-readable binding to a workflow
check, exact revision, checker identity, or content digest; it cannot authorize
omission. Only local checks on an exact clean head checkout can do so. Missing,
changed, or untrusted checkers and incomplete inputs keep generated text visible.

Each mapped source/section pair is a `source_section_obligation`. If the
section text is unchanged between the merge-base and head, the report marks
`needs_human_rationale`; this is a review obligation, not a mechanical failure
or a demand for a meaningless Markdown edit. An optional JSON file can record a
section-specific rationale:

<!-- example: id=impact-ack-invocation classification=illustrative reason="Revision and file values are supplied by the caller." -->
```bash
python3 scripts/docs.py impact --base BASE --head HEAD --format json \
  --acknowledgments /path/outside/the/checkout/acknowledgments.json
```

The file has `schema_version: 1` and an `acknowledgments` array. Each entry must
name `source_path`, `section_id`, and a non-empty `rationale`, plus exact
`source_revision`/`source_sha256`, `document_revision`/`document_sha256`/
`document_section_sha256`, and `base_document_revision`/
`base_document_sha256`/`base_document_section_sha256` values copied from the
report's obligation identity. The CLI independently resolves those revisions
and digests from Git objects; stale identities, changed prose, or unmapped
sections are errors. The report hashes the caller-supplied file and labels the
rationale as such; it is not proof of semantic correctness and does not replace
code review.

Generated equality proves byte-for-byte synchronization only, not that prose
or behavior is semantically correct. `docs-examples` verifies only declared
fixtures and parser-check-only examples; it does not establish that an example
is the right guidance. Unmapped source paths, changed policy/checker/generator
or fixture assertions, missing sections, renames/deletions, and unsupported
Markdown require raw review. The packet is a companion to the complete code
review and never removes code units from `do-harness pr review`.

The packet has a 24,000-byte ceiling. It is never truncated: an oversized or
incomplete packet reports an explicit fallback to the raw documentation diff
and full code review. PR triage consumes it only for the exact reviewed
revisions, with trusted checks, and when it is strictly smaller than the
equivalent full-document input; otherwise triage uses the raw input.
`scripts/docs-review-benchmark.py` reports candidate packet bytes and effective
review-input bytes, counting raw input for every fallback case. It measures
process/tool calls over generated-only, narrative-only, mixed, rename/delete,
policy-tampering, and unmapped fixtures; it reports tokens only with a named
tokenizer.

## What a green check does not prove

- `docs-links` proves target and anchor existence only. It never fetches
  anything: `http(s)://`, `mailto:`, `tel:`, `ftp://`, `data:`, protocol-relative
  `//`, and site-absolute `/` paths are skipped, except same-repository GitHub
  blob/raw links, which are normalized to local paths and checked.
- Anchors are verified for a link's own document and for targets ending in
  `.md`; an anchor into a non-markdown target is not checked, and `#top` always
  passes.
- A target that exists but is the wrong file passes, and so does a link whose
  visible text no longer describes its destination.
- Classification decides coverage. `historical`, `fixture`, and `vendored`
  documents — `docs/releases/**`, `tests/fixtures/**`, `.agents/skills/**`,
  `crates/do-harness/templates/**`, `vendor/**` — are neither link-checked nor
  example-checked; `tests/fixtures/doc_tools/**` in particular holds
  intentionally broken links. A markdown file outside `docs/` that is not a
  README and matches no `maintained` rule is not discovered at all.
- A relative link that resolves from a canonical file's own directory but not
  from a canonical alias's directory is printed as a warning, not a failure.
- `docs-sync` proves the marked regions only. Prose between and around them is
  not generated and is not verified.
- `docs-examples` runs the commands it is told to run. Blocks classified
  `illustrative` or `historical` are never executed, so a command that no
  longer works can still sit in one of them. A fixture that declares
  `platforms` is not executed on a host outside that list, and the skip is
  printed rather than silently accepted — on Windows, the local installer
  example reports this instead of running.
- Nothing here renders a website: redirects, external rendering, and external
  URL liveness are out of scope by design, because the checks must stay
  network-free to gate a pull request.

## Recovery

- Generated regions: run `python3 scripts/docs.py sync --write`, inspect that
  diff, and re-run the check. Writing is refused while `CI` is set, and a
  structural failure needs the marker pair added or removed by hand first — the
  report names the exact file, region, and command.
- Examples: repair the example named in the check output. A stale
  `displayed_command` means the fence or the manifest must change; an exit-code
  or `file_assertions` mismatch means the document and the CLI disagree, and
  one of them is wrong; an unmarked fence needs its marker.
- Links: repair the path or anchor named in the check output — re-point a moved
  file, re-anchor a renamed heading, or add an explicit anchor when the target
  heading must keep its title.
- When the checker itself is wrong, fix the tooling and its tests
  (`scripts/doc_tools/**`, `scripts/test_doc_tools.py`) instead of weakening a
  document, and express a deliberate exemption as a classification in
  `contracts.toml` rather than by disabling a check.
- After any repair, re-run the three checks, or scope a verify run to the
  sensors: `do-harness verify --only docs-sync --only docs-examples --only
  docs-links`.
