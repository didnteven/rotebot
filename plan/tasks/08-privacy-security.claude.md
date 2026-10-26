Track: PRIVACY, SECURITY AND THREAT MODEL. Output: plan/research/08-privacy-security.md
This product records everything on screen and then runs AI-written code. Do a real threat model (STRIDE-style, but readable):
- Data at rest: encryption (SQLCipher / per-file keys in Keychain / Windows DPAPI), what is stored (frames, OCR text, events, episodes), retention defaults, secure delete, export.
- Data in motion: what exactly goes to Meta's API, redaction before upload (on-device PII detection: regex + local model; masking regions in screenshots), never sending raw frames by default (send OCR/AX text + redacted crops?), user-visible "what was sent" log.
- Exclusions: per-app/per-site blocklist (password managers, banking, private browsing, messaging), auto-pause on secure input, incognito detection, sensitive-window detection, meeting/screen-share awareness.
- PROMPT INJECTION FROM SCREEN CONTENT: an email or web page the user views can contain instructions that later reach the code-generating agent. Design defenses: content is data never instructions, provenance tags, no tool access in the analysis stage, human approval before any generated code runs, capability-scoped manifests.
- Generated code execution: permission manifest per micro-app, sandbox options per OS (macOS sandbox-exec is deprecated but works; App Sandbox for helper apps; containers; Deno-style permissions; WASM/wasmtime), network allowlists, secrets handling (Keychain refs, never in code), audit log, kill switch, signing generated artifacts.
- Malware-look risk: an app that records screen + keystroke timing + runs scripts looks like spyware to EDR/AV and to Apple notarization review. How to stay on the right side (transparency, no keystroke content, clear indicators).
- Legal: GDPR/CCPA, recording third parties (video calls, other people's messages), workplace monitoring laws, Meta API terms on sensitive data.
Output a prioritized list of security requirements (MUST for MVP / later).
