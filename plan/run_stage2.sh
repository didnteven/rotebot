#!/bin/zsh
cd /Users/todd/StudioProjects/macroscope
S=plan/logs/_stage2.txt; : > $S
until grep -q ALL_DONE plan/logs/_status.txt; do sleep 30; done
echo "research done $(date +%T)" >> $S
SONNET=(claude --model claude-sonnet-5-5 --effort high --permission-mode acceptEdits --allowedTools "Read,Write,Edit,Glob,Grep")

# --- A: synthesis (Sonnet) ---
"${SONNET[@]}" -p "You are the lead planner. Read plan/BRIEF.md, then EVERY file in plan/research/ (15 reports by Codex and Claude agents; 11/12 are two independent architecture takes, 13/14 two UX takes). Write the single master plan to plan/PLAN.md.

It must be a decision document a solo developer can build from, not a summary of the reports. Where reports disagree (stack choice, UX model, cost estimates, MVP scope), pick one, say why in a sentence, and note the rejected option. Preserve concrete detail that matters for building (schemas, manifest example, process diagram, MSP mapping, API-key flow, permission sequence, wireframes) instead of pointing to the reports; link to a report section only for depth.

Required sections:
0. One-paragraph summary + a 'Key decisions' table (decision / choice / why / rejected alternative).
1. Product: persona(s), the v1 wedge, the first 10 workflows, MVP in/out, success metrics.
2. UX: principles, information architecture, ASCII wireframes of the key screens (onboarding+permissions, menu bar, Discoveries, Opportunity detail, Builder with Muse transcript + terminal toggle, Automation run/trust/repair, Privacy center), and the end-to-end flow discovery -> build -> test -> trust -> repair. Fold in the best ideas from the contrarian UX take.
3. Architecture: stack, process model diagram, IPC, storage layout, repo tree.
4. Meta models + Muse integration: model routing table, the provider abstraction, how muse serve is hosted, MSP-to-UI mapping, and an 'API-key auth' subsection stating exactly what was VERIFIED locally (see BRIEF.md and plan/research/04-muse-msp.md and plan/probes/) versus still unverified. All Meta usage is API-key based; make sure every path works with that.
5. Capture: macOS in detail, Windows plan, cross-platform observer interface, permissions, budgets.
6. Pattern engine: event schema, episodes, mining, scoring, Opportunity object.
7. Micro-apps: SDK/DSL vs full code, manifest example, runtimes, sandboxing, testing, repair.
8. Privacy & security: threat model summary, prompt-injection defences, MUST-for-MVP list.
9. Cost & performance budgets, evaluation plan and release gates.
10. Roadmap: milestones M0..Mn for macOS first, then Windows, each with deliverable, exit criteria, and an estimate; the first two weeks in detail (including a spike that proves capture + muse serve + API key end to end). How Windows is verified without the owner testing it.
11. Risks (ranked, with mitigations and kill criteria).
12. Open questions for the owner (deduplicated, only real decisions, each with your recommended answer).
Mark unverified external facts as such. Aim for 9,000-14,000 words. Write the file in pieces if needed." > plan/logs/A-synthesis.sonnet.log 2>&1
echo "A exit=$? $(date +%T) size=$(wc -c < plan/PLAN.md 2>/dev/null)" >> $S

# --- B: reviews in parallel (Codex + Sonnet) ---
codex --search exec -C . -s workspace-write --skip-git-repo-check -c model_reasoning_effort=high -o plan/logs/B-codex.last.md "Read plan/BRIEF.md and plan/PLAN.md. You may consult plan/research/* and plan/probes/* to check claims. Act as a hard-nosed technical reviewer and red team. Write plan/reviews/codex-review.md with: (1) the 10 most important problems, ranked, each with evidence and a concrete fix (wrong or unverified technical claims about macOS/Windows APIs, Meta/Muse APIs and API-key auth, muse serve/MSP; infeasible scope for a solo dev; missing pieces; security holes; cost errors; contradictions between sections); (2) anything that would block the first two-week spike; (3) things the plan gets right that must not be changed in revision. Use web search to verify API claims where in doubt. Do not edit PLAN.md." > plan/logs/B-codex-review.log 2>&1 &
"${SONNET[@]}" -p "Read plan/BRIEF.md and plan/PLAN.md (consult plan/research/13-ux.md and 14-ux-codex.md, 01-product.md as needed). Review the plan as a product and UX lead. Write plan/reviews/sonnet-review.md: (1) the 10 most important problems ranked, each with a concrete fix: is the v1 wedge sharp enough, would a first user get value in the first 30 minutes, are the screens and flows coherent and macOS-native, are the trust/permission moments handled well, is the Builder (Muse transcript, approvals, terminal toggle) understandable to a non-developer, is the scope achievable for one developer; (2) any wireframe that should be redrawn, with the redrawn ASCII; (3) what must not be changed. Do not edit PLAN.md." > plan/logs/B-sonnet-review.log 2>&1 &
wait
echo "B done $(date +%T) codex=$(wc -c < plan/reviews/codex-review.md 2>/dev/null) sonnet=$(wc -c < plan/reviews/sonnet-review.md 2>/dev/null)" >> $S

# --- C: revision (Sonnet) ---
"${SONNET[@]}" -p "Read plan/BRIEF.md, plan/PLAN.md, plan/reviews/codex-review.md and plan/reviews/sonnet-review.md. Revise plan/PLAN.md in place to address every review point you agree with (verify against plan/research and plan/probes when reviewers disagree with the plan). Keep the structure and the concrete detail; do not shorten sections that the reviews said to keep. Then write plan/reviews/CHANGES.md: a table of each review point -> accepted/rejected -> what changed (or why rejected)." > plan/logs/C-revise.sonnet.log 2>&1
echo "C exit=$? $(date +%T) size=$(wc -c < plan/PLAN.md)" >> $S
echo STAGE2_DONE >> $S
