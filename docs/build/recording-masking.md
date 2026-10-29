# Recording masking

This is an offline design review based on established screenshot redaction
practice, not a web literature search (network access was prohibited). The useful
pattern is OCR + recognisers/validators + field/layout context + optional local NER
+ temporal region tracking. OCR alone and regex-only classification of individual
boxes are sensitive to recognition and segmentation changes.

Adopted in pure Rust:

- Normalised lines with UTF-8 byte provenance, whitespace tolerance, and compact
  box-boundary fallback. URL lines supply context for numeric path fragments;
  printable ASCII percent escapes are decoded with source-span mapping. Numeric
  OCR O/0 and l/1 repairs are limited to numeric candidates.
- Sensitive spans for email (including Unicode), Luhn cards, structural IBAN,
  SSN, NZ accounts, IRD, phones, known API key prefixes, JWT, and opaque hex/base64
  candidates. IRD checksum validation helps distinguish unlabelled short numbers.
  Token character diversity/distribution replaces the old length-only rule;
  readable paths/words with a year suffix do not qualify merely by length.
- Secret-named assignments and field labels associate values to the right or on
  the nearest aligned row below, bounded by neighbouring fields. Labels remain
  visible. Ordinary dates, amounts, short numbers and common navigation phrases
  are excluded without overriding an explicit sensitive field value.
- Byte spans project to character-proportional sub-box rectangles, rounded
  outward and padded with the existing margin. This estimates glyph positions;
  proportional fonts and imperfect OCR boxes still need real-window inspection.
- Session-only memory tracks normalised text and regions for at most three newer
  sequence numbers / three seconds. Small edits (10%, capped at four characters),
  O/0/l/1 differences, splits/merges and <=6 px position jitter retain an envelope
  around previous masks. Changed app, dimensions, layout/text, or expiry clears
  matching authority. Fresh detection replaces stored geometry to bound growth.
  History has no Debug/Serialize, is never included in a preview, and updates only
  after the frame passes all checks and encoding.

Rejected: cloud OCR/PII/LLM services (privacy and no network); masking whole lines
or any long alphanumeric run (false positives); requiring valid IBAN checksums
before masking (OCR-induced privacy misses); guessing names/addresses with broad
regexes (unacceptable precision). NSDataDetector phone/address/link types and
NLTagger personalName were considered, but no offline accuracy/model-availability
proof was established in this change, so they are not enabled. Swift is unchanged.
General NER, unlabelled ordinary-word passwords, missing OCR, and novel secret
formats remain limitations; the fabricated corpus is not a universal PII guarantee.

Existing secure/manual regions, geometry/pixel/metadata limits, error propagation,
opaque burn-before-PNG, overlapping OCR removal, AX clearing and title omission
remain. A partially masked OCR box is entirely removed from text metadata.
Uncertain frames remain kept and marked unverified; secure-input gaps still drop.
No capture content logging, disk staging, network access, new dependencies, UI
changes or changes to browser capture eligibility were introduced.

The test-only frozen old detector and the new pipeline run on the same fabricated
browser/banking/email/spreadsheet/terminal lines and blocks, including split boxes,
extra spaces, case changes and O/0/l/1 variants. Metrics classify each line/block;
additional tests inspect span bounds, decoded black pixels, neighbouring labels,
multi-field layouts, temporal expiry/isolation and invalid adapter ranges.

248 cases: 122 must mask and 126 harmless.

| Detector | Precision | Recall | Harmless specificity | TP / FP / TN / FN |
| --- | --- | --- | --- | --- |
| Old | 78.31% | 53.28% | 85.71% | 65 / 18 / 108 / 57 |
| New | 100% | 100% | 100% | 122 / 0 / 126 / 0 |

The corpus asserts 100% must-mask recall, >=95% conventional precision and >=95%
harmless specificity (the last measure makes false positives on harmless examples
explicit). These are synthetic test results, not measured production accuracy.

Verification uses the SSD target directory with its exact trailing-space volume
name and Cargo offline mode:

```sh
source ~/.cargo/env
export CARGO_TARGET_DIR="/Volumes/250GB SAMSUNG 970 EVO PLUS /dev-tools/macroscope-claude-targets/mask"
export CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true
cargo test -p macroscope-app-core recording -- --nocapture
cargo test -p macroscope-app-core
cargo test -p macroscope-app-core --no-fail-fast
```

Recording: **41 passed, 1 failed**. The unchanged gateway fixture cannot bind
its localhost Wiremock socket (`Operation not permitted`). A run excluding that
fixture passes all 41 remaining recording tests. Full default-feature suite:
**142 passed, 29 failed, 1 ignored** with `--no-fail-fast`; all failures are denied
localhost binds, including the parent test whose child fixture fails that way.
The plain full command also ran (89 passed, 1 socket failure before Cargo stopped).
No sandbox/network escalation was attempted. Formatting and `git diff --check`
pass. Test logs contain build output and fabricated fixtures only.

Swift tests were not required because no Swift files changed. Live signed-app
capture, on-device NER and variable-font geometry accuracy were not verified.
