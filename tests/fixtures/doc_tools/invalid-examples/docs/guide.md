# Guide

The example below is classified `executable` but names a fixture that
`tests/fixtures/docs-examples/manifest.toml` does not declare, so the example
checker cannot run it.

<!-- example: id=broken_example classification=executable fixture=absent_fixture -->
```bash
do-harness verify --format json
```
