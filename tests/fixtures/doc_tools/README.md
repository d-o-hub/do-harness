# Test fixtures for doc_tools tests

Each directory is a miniature repository root that
`python3 scripts/docs.py <check> --check --root <dir>` (the
`docs-sync`/`docs-examples`/`docs-links` sensors) is run against, so the
`docs/contracts.toml` inside one is the policy that directory is classified
under.

- `valid/`: a healthy tree; `scripts/test_doc_tools.py` asserts the checkers
  accept it.
- `invalid-links/`: a broken relative path and a broken cross-document anchor.
- `invalid-examples/`: an `executable` example naming a fixture that its
  manifest does not declare.
- `empty/`: a policy and zero documents, so discovery cannot pass vacuously.
- `malformed-generated/`: an unterminated generated marker pair, with
  `bin/cargo` standing in for the maintenance test that rejects it (see
  `TestDocsChecksCanFail` in `scripts/test_doc_tools.py`).
