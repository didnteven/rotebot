import { BotStatus, ThemePicker } from "./Bot";
import { useEffect, useRef, useState } from "react";
import type {
  AppInfo,
  Backend,
  ModelAccessStatus,
  ModelRoute,
  ShowState,
} from "./backend/types";
import { Field, Notice, useAction } from "./ui";
import { CustomEndpointCard } from "./CustomEndpointCard";
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
  const [confirmation, setConfirmation] = useState("");
  /** Only the setup for the service being used, or being set up, is shown. */
  const [opened, setOpened] = useState<ModelRoute | null>(null);
  const active = status.selectedRoute ?? status.effectiveRoute;
  const configured: Record<ModelRoute, boolean> = { muse_plan: status.muse.kind !== "not_signed_in" && status.muse.kind !== "missing", api_key: status.apiKey.present, custom_endpoint: status.customEndpoint.tokenPresent || !!status.customEndpoint.host, offline_demo: false };
  const showSetup = (route: ModelRoute) => opened === route || active === route || configured[route];
  const chooser = useRef<HTMLFieldSetElement>(null);

  useEffect(() => { chooser.current?.focus({ preventScroll: true }); }, []);
  const [login, setLogin] = useState(false);
  useEffect(() => { void backend.loginEnabled().then(setLogin).catch(() => {}); }, [backend]);
  const key = useRef<HTMLInputElement>(null);
  const { act, error, busy } = useAction();
  const muse = status.muse;
  function enterKey() {
    if (info.platform === "macos" && !info.mock) void act(async()=>update(await backend.enterApiKeyNative()));
    else { setOpened("api_key"); setEntering(true); document.getElementById("meta-key-setup")?.scrollIntoView?.({block:"start"}); }
  }
  const routes: {route:ModelRoute;label:string;disabled:boolean;reason:string;fix?:string;repair?:()=>void}[] = [
    {route:"muse_plan",label:"Muse plan",disabled:muse.kind!=="signed_in",reason:muse.kind==="signed_in"?"Ready":muse.kind==="missing"?"Not installed":muse.kind==="not_signed_in"?"Not signed in":"Check sign-in",fix:"Sign in…",repair:()=>{setOpened("muse_plan");void act(async()=>update(await backend.refreshMuseStatus()));}},
    {route:"api_key",label:"Meta API key",disabled:!status.apiKey.present,reason:status.apiKey.present?"Ready":"Needs a key",fix:"Add key…",repair:enterKey},
    {route:"custom_endpoint",label:"Custom endpoint",disabled:!status.customEndpoint.tokenPresent || status.customEndpoint.lastTest?.status!=="ok",reason:status.customEndpoint.tokenPresent && status.customEndpoint.lastTest?.status==="ok"?"Ready":"Not set up",fix:"Set up…",repair:()=>{setOpened("custom_endpoint");setTimeout(()=>{document.getElementById("custom-endpoint-setup")?.scrollIntoView?.({block:"start"});document.getElementById("custom-endpoint-setup")?.querySelector("input")?.focus();});}},
    {route:"offline_demo",label:"Offline demo (samples only)",disabled:false,reason:"Ready"},
  ];
  async function select(route:ModelRoute) {
    const next = await backend.selectRoute(route);
    update(next);
    setConfirmation(next.effectiveRoute === route ? `Builds now use ${routeLabel(route)}` : next.blockedReason ?? "This route is currently unavailable.");
  }
  return (
    <section>
      <p className="muted">Your tools can use this AI service and the services you connect. Each result shows what it used.</p>
      <fieldset ref={chooser} id="model-route-chooser" tabIndex={-1} disabled={busy}>
        <legend>Which model builds your tools</legend>
        {routes.map((r) => (
          <div className="model-route-row" key={r.route}>
            <label className="choice">
              <input
                type="radio"
                aria-describedby={`route-state-${r.route}`}
                name="route"
                value={r.route}
                checked={
                  (status.selectedRoute ?? status.effectiveRoute) === r.route
                }
                disabled={r.disabled}
                onClick={() => {
                  if (
                    status.selectedRoute === null &&
                    status.effectiveRoute === r.route
                  )
                    void act(async () =>
                      select(r.route),
                    );
                }}
                onChange={() =>
                  void act(async () =>
                    select(r.route),
                  )
                }
              />
              {r.label}
            </label>
            <p id={`route-state-${r.route}`}>{r.reason}</p>
            {r.disabled && r.fix && <button onClick={r.repair}>{r.fix}</button>}
          </div>
        ))}
      </fieldset>
      {confirmation && <p role="status">{confirmation}</p>}
      {info.debugBuild && info.secretStore === "file" && (
        <p className="muted">
          Development build: secrets are stored in a private file in Macroscope's data folder instead of the Keychain.
        </p>
      )}
      <Notice error={error} />
      {busy && <BotStatus label="Checking model access…" />}
      {showSetup("custom_endpoint") && <div id="custom-endpoint-setup"><CustomEndpointCard backend={backend} status={status.customEndpoint} update={update} /></div>}
      <div className="card-grid">
        {showSetup("api_key") && <article className="card" id="meta-key-setup">
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
                enterKey();
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
                  setEntering(false);
                });
              }}
            >
              <Field label="Paste your Meta API key">
                <input autoFocus ref={key} type="password" autoComplete="off" required />
              </Field>
              <button disabled={busy} type="submit">
                Save key
              </button>
            </form>
          )}
        </article>}
        {showSetup("muse_plan") && <article className="card">
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
              <label className="choice">
                <input
                  type="checkbox"
                  checked={status.museUnverifiedOk}
                  disabled={busy}
                  onChange={(event) => {
                    const ok = event.target.checked;
                    void act(async () => update(await backend.setMuseUnverifiedOk(ok)));
                  }}
                />
                Allow Muse builds before file-access confinement is verified
              </label>
              <p>
                Muse runs with its shell and file writes turned off in a separate build folder, but its file-reading tools have not yet been proven to stay inside that folder.
              </p>
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
        </article>}
      </div>
      {status.blockedReason && (
        <p
          className={
            status.effectiveRoute === null
              ? "notice notice-warning"
              : "notice notice-info"
          }
        >
          {status.blockedReason}
        </p>
      )}
      <h2>Preferences</h2>
      <label className="choice"><input type="checkbox" checked={login} disabled={busy} onChange={e => { const enabled=e.target.checked; void act(async()=>{await backend.setLoginEnabled(enabled);setLogin(enabled);}); }} />Open Macroscope at login so schedules run</label>
      <ThemePicker />
      <details><summary>About</summary><p className="muted">
        Version {info.version} · {info.platform}
        {info.mock
          ? " · Browser demo; no external calls or persistent files"
          : ""}
      </p>
      <p className="muted">Record a task: {show.kind === "unavailable" ? show.reason : "Available"}</p>
      <p className="muted">Macroscope never signs in or out of Muse for you and never changes Muse's saved login.</p>
      </details>
    </section>
  );
}
