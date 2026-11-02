import { useEffect, useState } from "react";
import { BotAvatar, type BotState } from "./Bot";
import type { BuildPhase, BuildState } from "./backend/types";

const phases: Record<BuildPhase, { title: string; detail: string; pose: BotState }> = {
  queued: { title: "Getting ready", detail: "Your build is queued. Your bot will start as soon as it can.", pose: "waiting" },
  writing: { title: "Building your tool", detail: "Turning your plan into a program that can do the job again.", pose: "building" },
  candidate: { title: "Preparing the checks", detail: "A first version is ready. Preparing to test it against your data.", pose: "working" },
  checking: { title: "Testing your tool", detail: "Running the checks before you review the result.", pose: "working" },
  ready: { title: "Ready for your review", detail: "The build is ready. Check the result below before saving your tool.", pose: "success" },
  failed: { title: "This build needs attention", detail: "Review what happened below and choose how to continue.", pose: "error" },
  stopped: { title: "Build stopped", detail: "Build stopped. Your plan is saved. Change the request or try again.", pose: "paused" },
};
const stages = [
  { label: "Queue", phases: ["queued"] },
  { label: "Build", phases: ["writing", "candidate"] },
  { label: "Test", phases: ["checking"] },
  { label: "Review", phases: ["ready"] },
];

/** Display only progress reported by the host; retries can move back to an earlier stage. */
export function BuildProgress({ build, seed, name, busy, stop }: {
  build: BuildState; seed: string; name: string; busy: boolean; stop: () => void;
}) {
  const active = !["ready", "failed", "stopped"].includes(build.phase);
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [active, build.buildId]);
  const first = build.events[0];
  const latest = build.events.at(-1);
  const seconds = first ? Math.max(0, Math.floor(((active ? now : latest?.atMs ?? first.atMs) - first.atMs) / 1000)) : null;
  const elapsed = seconds === null ? null : seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  const presentation = phases[build.phase];
  const interrupted = build.phase === "failed" || build.phase === "stopped";
  const lastWorkPhase = [...build.events].reverse().find(event => !["failed", "stopped"].includes(event.phase))?.phase;
  const current = stages.findIndex(stage => stage.phases.includes(interrupted ? lastWorkPhase ?? "" : build.phase));

  return <section className={`build-scene build-scene-${build.phase}`} aria-label="Build progress">
    <div className="build-portrait" aria-hidden="true">
      <span className="build-orbit build-orbit-outer" />
      <span className="build-orbit build-orbit-inner" />
      <BotAvatar seed={seed} state={presentation.pose} size={184} />
      <span className="portrait-ground" />
    </div>
    <div className="build-scene-content">
      <p className="eyebrow">Your bot’s workshop</p>
      <div role="status" aria-live="polite" aria-atomic="true">
        <h3>{presentation.title}</h3>
        <p className="build-description">{presentation.detail}</p>
      </div>
      <p className="build-tool-name">{name || "Your new tool"}</p>
      <ol className="build-stages" aria-label="Build stages">
        {stages.map((stage, index) => {
          const reached = build.events.some(event => stage.phases.includes(event.phase));
          const complete = current > index && (reached || build.phase === "ready");
          const state = complete ? "complete" : current === index ? interrupted ? "interrupted" : "current" : "pending";
          const status = complete ? "complete" : current !== index ? "pending" : interrupted ? build.phase : build.phase === "ready" ? "ready for your review" : "in progress";
          return <li key={stage.label} className={`build-stage-${state}`} aria-current={current === index ? "step" : undefined}>
            <span className="build-stage-marker" aria-hidden="true">{complete ? "✓" : interrupted && current === index ? "!" : index + 1}</span>
            <span>{stage.label}<span className="sr-only">: {status}</span></span>
          </li>;
        })}
      </ol>
      <div className="build-scene-footer">
        {elapsed && <span className="build-elapsed">{active ? "Elapsed" : "Duration"} · {elapsed}</span>}
        {active && <button className="quiet" disabled={busy} onClick={stop}>Stop build</button>}
      </div>
      {active && latest && <details className="build-latest"><summary>Latest update</summary><p>{latest.message}</p></details>}
    </div>
  </section>;
}
