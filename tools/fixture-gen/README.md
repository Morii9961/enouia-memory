# Fixture and schema generators

Python 3 (standard library only). They produced the committed JSON Schemas and synthetic fixtures. The committed files remain the source of truth; rerun these after changing a contract, then review the diff.

```text
python tools/fixture-gen/gen_schemas.py      # contracts/{memory,context,provider}
python tools/fixture-gen/gen_ipc.py          # contracts/ipc/memory-v1.schema.json
python tools/fixture-gen/gen_fixtures.py     # tests/fixtures/memory (records, sets, manifests)
python tools/fixture-gen/gen_ipc_fixtures.py # tests/fixtures/memory/ipc-manifest.json (reads records/)
```

All generated content is synthetic. It is not anyone's real memory or chat.
