import { useEffect, useState } from "react";
import type { Backend, ShowState } from "./backend/types";
import { Notice, useAction } from "./ui";

export function ShowPanel({
  backend,
  state,
  update,
  close,
}: {
  backend: Backend;
  state: ShowState;
  update: (s: ShowState) => void;
  close: () => void;
}) {
  const { act, busy, error } = useAction();
  return (
    <section className="card">
      <button onClick={close}>Back</button>
      <h2>Show a task</h2>
      <Notice error={error} />
      {state.kind === "unavailable" ? (
        <p>{state.reason}</p>
      ) : state.kind === "recording" ? (
        <p>
          Recording {state.paused ? "paused" : "in progress"}. Use the timer bar
          to pause or stop.
        </p>
      ) : state.kind === "finished" ? (
        <>
          <h3>Recording summary</h3>
          <p>Duration: {state.durationSeconds} seconds</p>
          <p>Apps seen: {state.appsSeen.join(", ") || "No apps seen"}</p>
          <p>
            Connection suggestions appear above. Connecting grants no tool
            access.
          </p>
          <button
            onClick={() =>
              void act(async () => update(await backend.startShow()))
            }
            disabled={busy}
          >
            Record another task
          </button>
        </>
      ) : (
        <>
          <p>One task · maximum 5 minutes</p>
          <p>
            Records which apps you use and whether a window matches a known
            service like Gmail — never screen pixels, keystrokes, or window
            text.
          </p>
          <p>
            Excluded: password managers, finance apps, private browser windows,
            terminals. No screen access or Input Monitoring is requested.
          </p>
          <p>
            Accessibility: {state.accessibility}. Accessibility permits local
            app and service matching; raw window titles are discarded. Recording
            requires your explicit start.
          </p>
          {state.accessibility !== "granted" && (
            <p>
              Enable Accessibility for Macroscope in macOS System Settings, then
              check again. Tell works without it.
            </p>
          )}
          <div className="actions">
            <button
              onClick={() =>
                void act(async () => update(await backend.showState()))
              }
            >
              Check Accessibility again
            </button>
            <button
              className="primary"
              disabled={busy || state.accessibility !== "granted"}
              onClick={() =>
                void act(async () => update(await backend.startShow()))
              }
            >
              Start recording
            </button>
          </div>
        </>
      )}
    </section>
  );
}
export function RecordingBar({
  backend,
  state,
  update,
}: {
  backend: Backend;
  state: Extract<ShowState, { kind: "recording" }>;
  update: (s: ShowState) => void;
}) {
  const [now, setNow] = useState(Date.now());
  const { act, busy, error } = useAction();
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  const seconds = Math.min(
    state.maxSeconds,
    Math.max(0, Math.floor((now - state.startedAtMs) / 1000)),
  );
  return (
    <div className="recording-bar">
      <strong>{state.paused ? "Paused" : "Recording"}</strong>
      <span role="timer" aria-label="Recording timer">
        {Math.floor(seconds / 60)}:{String(seconds % 60).padStart(2, "0")} /
        5:00
      </span>
      <span className="sr-only" aria-live="polite">
        {state.paused
          ? "Recording paused"
          : `${Math.floor(seconds / 60)} minutes recorded`}
      </span>
      <span>{state.matches} service matches</span>
      <button
        disabled={busy}
        onClick={() =>
          void act(async () => update(await backend.pauseShow(!state.paused)))
        }
      >
        {state.paused ? "Resume" : "Pause"}
      </button>
      <button
        disabled={busy}
        onClick={() => void act(async () => update(await backend.stopShow()))}
      >
        Stop
      </button>
      <Notice error={error} />
    </div>
  );
}
