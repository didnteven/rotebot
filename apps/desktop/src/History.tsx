import { BotStatus } from "./Bot";
import { useEffect, useState } from "react";
import type {
  ActivityItem,
  Backend,
  ModelAccessStatus,
  PrivacyOverview,
  DeletePreview,
  Receipt,
} from "./backend/types";
import { DataTable, Notice, ReceiptCard, useAction } from "./ui";

export function Activity({ backend }: { backend: Backend }) {
  const [items, setItems] = useState<ActivityItem[] | null>(null),
    [receipts, setReceipts] = useState<Receipt[]>([]);
  const { act, busy, error } = useAction();
  const load = async () => {
    const [a, r] = await Promise.all([
      backend.listActivity(),
      backend.listReceipts(),
    ]);
    setItems(a);
    setReceipts(r);
  };
  useEffect(() => {
    void act(load);
  }, [backend]);
  const entries = [
    ...(items ?? []).map((item) => ({
      at: item.atMs,
      id: item.id,
      content: (
        <article className="card">
          <h2>{item.title}</h2>
          <p>{item.detail}</p>
          <time dateTime={new Date(item.atMs).toISOString()}>
            {new Date(item.atMs).toLocaleString()}
          </time>
        </article>
      ),
    })),
    ...receipts.map((receipt) => ({
      at: receipt.atMs,
      id: `receipt-${receipt.runId}`,
      content: <ReceiptCard receipt={receipt} backend={backend} />,
    })),
  ].sort((a, b) => b.at - a.at);
  return (
    <section>
      <Notice error={error} />
      {busy && <BotStatus label="Loading activity…" />}
      {error && <button onClick={() => void act(load)}>Reload activity</button>}
      {items && entries.length === 0 && (
        <p>No activity yet. Build or run a tool to see its history here.</p>
      )}
      <ul className="plain-list">
        {entries.map((e) => (
          <li key={e.id}>{e.content}</li>
        ))}
      </ul>
    </section>
  );
}
export function Privacy({
  backend,
  status,
  update,
  stopped,
}: {
  backend: Backend;
  status: ModelAccessStatus;
  update: (s: ModelAccessStatus) => void;
  stopped: () => Promise<void>;
}) {
  const [overview, setOverview] = useState<PrivacyOverview | null>(null),
    [stopping, setStopping] = useState(false);
  const { act, busy, error } = useAction();
  const [preview, setPreview] = useState<DeletePreview | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [includeTools, setIncludeTools] = useState(false);
  const [deleted, setDeleted] = useState(false);
  const emergency = useAction();
  const load = async () => {
    const [o, p] = await Promise.all([
      backend.privacyOverview(),
      backend.deletePreview(),
    ]);
    setOverview(o);
    setPreview(p);
  };
  useEffect(() => {
    void act(load);
  }, [backend]);
  return (
    <section>
      <Notice error={error} />
      {busy && !stopping && <BotStatus label="Updating privacy…" />}
      {error && <button onClick={() => void act(load)}>Reload privacy</button>}
      <h2>Cloud models</h2>
      <p>Choose whether Macroscope may send approved information to an online AI service. Local only stops cloud work; it does not stop tools that run on this Mac.</p>
      <Notice error={emergency.error} />
      <fieldset disabled={busy}>
        <legend>Cloud models</legend>
        {[false, true].map((local) => (
          <label className="choice" key={String(local)}>
            <input
              type="radio"
              name="egress"
              checked={
                local
                  ? status.egress === "local_only"
                  : status.egress === "cloud_allowed"
              }
              onChange={() =>
                void act(async () => {
                  setStopping(local);
                  try {
                    update(await backend.setLocalOnly(local));
                    await load();
                  } finally {
                    setStopping(false);
                  }
                })
              }
            />
            {local ? "Local only" : "Use cloud models"}
          </label>
        ))}
      </fieldset>
      {(stopping || status.egress === "stopping") && (
        <p role="status">Stopping cloud work…</p>
      )}
      <p>
        Transmitted requests cannot be recalled and may still incur charges.
      </p>
      <button
        className="danger"
        disabled={emergency.busy}
        onClick={() =>
          void emergency.act(async () => {
            await backend.emergencyStop();
            await stopped();
            await load();
          })
        }
      >
        Emergency stop
      </button>
      {emergency.busy && <BotStatus label="Stopping builds, tools, and recording…" />}
      <dl className="explanations">
        <dt>Pause recording</dt>
        <dd>Pauses observation only; builds continue.</dd>
        <dt>Stop builds</dt>
        <dd>Cancels active and queued builds; recording continues.</dd>
        <dt>Local only</dt>
        <dd>Stops all cloud model work; tools that run on this Mac continue.</dd>
        <dt>Emergency stop</dt>
        <dd>Stops builds, tools, and recording.</dd>
        <dt>Delete</dt>
        <dd>
          Removes the selected local history; provider copies cannot be
          recalled.
        </dd>
      </dl>
      <h2>Your data</h2>
      <p>
        Deleting history stops builds, recording, and runs, then removes the local
        items listed below. Your API key and connection tokens stay until you
        use Remove key or Disconnect. Files you selected and exported outputs
        stay in their own folders. Provider copies cannot be recalled.
      </p>
      {preview && (
        <DataTable
          caption="Local history to delete"
          columns={["Item", "Count"]}
          rows={preview.items.map((item) => [item.label, String(item.count)])}
        />
      )}
      {deleted && <p role="status">Local history deleted.</p>}
      {confirmDelete ? (
        <section aria-label="Confirm local history deletion" className="card">
          <h3>Delete local history?</h3>
          <p>
            Everything listed above will be deleted immediately. Cloud models
            will be set to Local only.
          </p>
          <label className="choice">
            <input
              type="checkbox"
              checked={includeTools}
              disabled={busy}
              onChange={(e) => setIncludeTools(e.target.checked)}
            />
            Also delete my tools
          </label>
          <p>
            {includeTools
              ? "All tools, custom bot designs, versions, permissions, runs and receipts, and their folders will also be deleted."
              : "Your saved tools, custom bot designs, versions, permissions, and run receipts will be kept."}
          </p>
          <div className="actions">
            <button
              disabled={busy}
              onClick={() =>
                void act(async () => {
                  await backend.deleteData({ includeTools });
                  if (includeTools) window.dispatchEvent(new Event("macroscope-bots-deleted"));
                  setConfirmDelete(false);
                  setDeleted(true);
                  setIncludeTools(false);
                  await stopped();
                  await load();
                })
              }
            >
              Delete history
            </button>
            <button disabled={busy} onClick={() => setConfirmDelete(false)}>
              Keep
            </button>
          </div>
        </section>
      ) : (
        <button
          disabled={busy || !preview}
          onClick={() =>
            void act(async () => {
              setPreview(await backend.deletePreview());
              setIncludeTools(false);
              setConfirmDelete(true);
              setDeleted(false);
            })
          }
        >
          Delete local history…
        </button>
      )}
      {overview && (
        <>
          <h2>Stored data</h2>
          <DataTable
            caption="Data stored by Macroscope"
            columns={["Data", "Count", "Size", "Expires"]}
            rows={overview.stored.map((s) => [
              s.label,
              String(s.count),
              s.count === 0 ? "—" : `${s.bytes.toLocaleString()} bytes`,
              s.expires,
            ])}
          />
          <h2>Sent data history</h2>
          {overview.outbound.length === 0 && <p>No sent data history.</p>}
          {overview.outbound.map((r) => (
            <article className="card" key={r.id}>
              <h3>{r.purpose}</h3>
              <p>
                {r.destination} · {new Date(r.atMs).toLocaleString()} ·{" "}
                {r.syntheticOnly
                  ? "Made-up example data only"
                  : "Contains selected data"}
              </p>
              <p>
                {r.purpose === "Connection read" ? "Connection read; no model call" : r.route === "custom_endpoint" ? "Custom endpoint call · price unknown" : r.costMicros === null
                  ? "Counts against your Muse plan"
                  : `$${(r.costMicros / 1e6).toFixed(2)}`}
              </p>
              <details>
                <summary>
                  {r.purpose === "Connection read" ? "Tool and approved argument template" : r.route === "custom_endpoint" ? "Byte counts and token usage (content is not logged)" : r.purpose === "Model step" ? "Route and byte counts (content is not logged)" : r.route === "muse_plan"
                    ? "Inputs supplied to Muse"
                    : "Exact data sent"}
                </summary>
                <pre tabIndex={0}>
                  {r.payloadText ?? "Sent text expired; history retained."}
                </pre>
              </details>
            </article>
          ))}
          <p>
            Delete sent data history removes {overview.outbound.length} local
            records and their saved text. Tools and exported CSVs
            remain.
          </p>
          <button
            disabled={busy || !overview.outbound.length}
            onClick={() =>
              void act(async () => {
                await backend.deleteOutboundHistory();
                await load();
              })
            }
          >
            Delete sent data history
          </button>
        </>
      )}
    </section>
  );
}
