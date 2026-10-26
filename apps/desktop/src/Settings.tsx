import { useRef, useState } from "react";
import type {
  AppInfo,
  Backend,
  ModelAccessStatus,
  ModelRoute,
  ShowState,
} from "./backend/types";
import { Field, Notice, useAction } from "./ui";
import { routeLabel } from "./backend/mock";

export function Settings({
  backend,
  info,
  status,
  show,
  update,
}: {
  backend: Backend;
  info: AppInfo;
  status: ModelAccessStatus;
  show: ShowState;
  update: (status: ModelAccessStatus) => void;
}) {
  const [entering, setEntering] = useState(false),
    [signingIn, setSigningIn] = useState(false),
    [docs, setDocs] = useState(false);
  const key = useRef<HTMLInputElement>(null);
  const { act, error, busy } = useAction();
  const muse = status.muse;
  const routes: {
    route: ModelRoute;
    label: string;
    disabled: boolean;
    reason: string;
  }[] = [
    {
      route: "api_key",
      label: "Meta API key (billed to your key)",
      disabled: !status.apiKey.present,
      reason: "Enter a key first.",
    },
    {
      route: "muse_plan",
      label: "Muse plan (uses your Muse allowance)",
      disabled: muse.kind !== "signed_in",
      reason: "Sign in to Muse Code and check again first.",
    },
    {
      route: "offline_demo",
      label: "Offline demo (no model)",
      disabled: false,
      reason: "",
    },
  ];
  return (
    <section>
      <h2>Model access</h2>
      <Notice error={error} />
      {busy && <p role="status">Checking…</p>}
      <div className="card-grid">
        <article className="card">
          <h3>Meta API key</h3>
          <p>
            {status.apiKey.present
              ? `Key ending …${status.apiKey.fingerprint ?? ""}`
              : "No key"}
          </p>
          <p>
            Last test:{" "}
            {status.apiKey.lastTest
              ? status.apiKey.lastTest.kind === "ok"
                ? "Passed"
                : status.apiKey.lastTest.kind.replaceAll("_", " ")
              : "Not tested"}
          </p>
          <div className="actions">
            <button
              disabled={busy}
              onClick={() => {
                if (info.platform === "macos" && !info.mock)
                  void act(async () =>
                    update(await backend.enterApiKeyNative()),
                  );
                else setEntering(true);
              }}
            >
              Enter key…
            </button>
            <button
              disabled={busy || !status.apiKey.present}
              onClick={() =>
                void act(async () => update(await backend.testApiKey()))
              }
            >
              Test key
            </button>
            <button
              disabled={busy || !status.apiKey.present}
              onClick={() =>
                void act(async () => update(await backend.removeApiKey()))
              }
            >
              Remove key
            </button>
          </div>
          {entering && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                const value = key.current?.value ?? "";
                if (key.current) key.current.value = "";
                void act(async () => {
                  update(await backend.saveApiKey(value));
                });
              }}
            >
              <Field label="Meta API key">
                <input ref={key} type="password" autoComplete="off" required />
              </Field>
              <button disabled={busy} type="submit">
                Save key
              </button>
            </form>
          )}
        </article>
        <article className="card">
          <h3>Muse Code plan</h3>
          {muse.kind === "missing" ? (
            <>
              <p>Muse Code not installed</p>
              <button onClick={() => setDocs(true)}>
                Show install guidance
              </button>
              {docs && (
                <div>
                  <p>
                    Install Muse Code using Meta's official developer
                    documentation, then check again.
                  </p>
                  <Field label="Install guidance URL">
                    <input
                      readOnly
                      value="https://dev.meta.ai/"
                      onFocus={(e) => e.target.select()}
                    />
                  </Field>
                  <button
                    disabled={busy}
                    onClick={() =>
                      void act(() =>
                        navigator.clipboard.writeText("https://dev.meta.ai/"),
                      )
                    }
                  >
                    Copy install guidance URL
                  </button>
                </div>
              )}
            </>
          ) : muse.kind === "not_signed_in" ? (
            <p>Not signed in</p>
          ) : muse.kind === "signed_in" ? (
            <>
              <p>Signed in</p>
              <p>{muse.planSummary ?? "Plan summary unavailable"}</p>
            </>
          ) : muse.kind === "unsupported_version" ? (
            <p>
              Unsupported Muse Code version: {muse.version}. Update Muse Code
              and check again.
            </p>
          ) : (
            <p>Unknown: {muse.message}</p>
          )}
          {muse.kind === "not_signed_in" && (
            <button
              disabled={busy}
              onClick={() =>
                void act(async () => {
                  await backend.openMuseSignIn();
                  setSigningIn(true);
                })
              }
            >
              Sign in with Muse…
            </button>
          )}
          {signingIn && muse.kind !== "signed_in" && (
            <p role="status">Finish signing in in Terminal, then check again</p>
          )}
          <button
            disabled={busy}
            onClick={() =>
              void act(async () => update(await backend.refreshMuseStatus()))
            }
          >
            Check again
          </button>
        </article>
      </div>
      <fieldset disabled={busy}>
        <legend>Build route</legend>
        {routes.map((r) => (
          <div key={r.route}>
            <label className="choice">
              <input
                type="radio"
                name="route"
                value={r.route}
                checked={status.selectedRoute === r.route}
                disabled={r.disabled}
                onChange={() =>
                  void act(async () =>
                    update(await backend.selectRoute(r.route)),
                  )
                }
              />
              {r.label}
            </label>
            {r.disabled && <p className="muted">{r.reason}</p>}
          </div>
        ))}
      </fieldset>
      <p>Effective route: {routeLabel(status.effectiveRoute)}</p>
      {status.blockedReason && <p className="notice">{status.blockedReason}</p>}
      <p>
        Macroscope never signs in or out of Muse for you and never changes
        Muse's saved login.
      </p>
      <p className="muted">
        Version {info.version} · {info.platform}
        {info.mock
          ? " · Browser demo; no external calls or persistent files"
          : ""}
      </p>
      <p>
        Show:{" "}
        {show.kind === "unavailable"
          ? show.reason
          : "Available · opt-in app metadata only"}
      </p>
    </section>
  );
}
