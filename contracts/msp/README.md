# Muse MSP schema snapshots

Observed/reviewed on 2026-10-27 (schemaVersion 1):

| Muse version | Schema fingerprint | Evidence |
| --- | --- | --- |
| 1.4.11 | `sha256:f282ad16e21520d0cda68cb190149b86e2b062eef1dd282749360c05aa3d1082` | Historical plan probes and `plan/reference/msp/` |
| 1.4.13-R6996.1 | `sha256:7fddaf646dbc317e5fcac0785e135eb04df14fbb0db1863089dfd6da21730e5b` | Vendored manifest/schema here; real echo conformance and `fixtures/msp/echo-1.4.13.jsonl` |

`plan/reference/msp/` is the historical 1.4.11 pin, not the schema of the
currently installed binary. The reviewer observed an additive version-1 diff:
12 new definitions, 72 changed definitions, none removed. A schema major version
other than 1 blocks builds. An unknown version-1 fingerprint requires isolated
echo conformance; changing a fingerprint is not sufficient to accept it.

Generated directly from the native executable, without invoking the updating
launcher:

```sh
/Users/todd/.local/bin/muse-bin-1.4.13-R6996.1 schema generate-json-schema --out contracts/msp/1.4.13-R6996.1/
```

The compiled known-fingerprint table records the producing version. Other
fingerprints need a successful app-owned acceptance record bound to the native
binary SHA-256, version and schema fingerprint. Records are written atomically;
a replacement binary requires a new check. Failed checks retain their failing
step and do not permit Meta work. Echo verifies protocol compatibility; the
separate Meta sandbox/containment gates still apply.

To refresh the real wire fixture through the conformance path:

```sh
MACROSCOPE_MUSE_IT=1 MACROSCOPE_RECORD_ECHO=1 cargo test -p macroscope-muse-client --test echo real_msp_echo -- --ignored
```

The helper replaces its complete private temporary root with
`<CONFORMANCE_ROOT>` before writing the JSONL fixture. It never uses the Meta
provider or the Muse launcher's update path.
