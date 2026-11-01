import { BotAvatar, BotStatus, botIdentity } from "./Bot";
import { ActionReview, ActionTechnicalDetails } from "./ActionReview";
import { useEffect, useState } from "react";
import type { Backend, ModelRoute, Program, TodayDay, TodayEntry } from "./backend/types";
import { FailureNotice, FileText, friendlyFailure, useAction, usedParts, UsedLine } from "./ui";

const statusWords: Record<TodayEntry["status"], string> = {
  needs_you: "Needs your OK",
  done: "Done",
  failed: "Failed - tap for details",
  staged: "Ready to review",
  skipped: "Skipped",
};

export function Today({ backend, tools, create, run = tools }: {
  backend: Backend;
  tools: () => void;
  create: () => void;
  run?: (toolId: string) => void;
}) {
  const [days, setDays] = useState<TodayDay[] | null>(null);
  const [programs, setPrograms] = useState<Record<string, Program | null>>({});
  const [route, setRoute] = useState<ModelRoute | null>(null);
  const { act, error, busy } = useAction();
  const refresh = async () => setDays(await backend.listToday());
  useEffect(() => {
    void act(refresh);
    const timer = setInterval(() => void act(refresh), 30_000);
    return () => clearInterval(timer);
  }, [backend]);
  const toolIds = [...new Set(days?.flatMap(d => d.entries.map(e => e.toolId)) ?? [])].join(",");
  useEffect(() => {
    let active = true;
    void backend.modelAccess().then(s => { if (active) setRoute(s.effectiveRoute); }).catch(() => {});
    void Promise.all(toolIds.split(",").filter(Boolean).map(async id => [id, await backend.getTool(id).then(t => t.program ?? null).catch(() => null)] as const))
      .then(entries => { if (active) setPrograms(Object.fromEntries(entries)); });
    return () => { active = false; };
  }, [backend, toolIds]);
  const needs = days?.flatMap(d => d.entries).filter(e => e.status === "needs_you") ?? [];
  const show = (entry: TodayEntry) => {
    const program = programs[entry.toolId];
    const time = new Date(entry.startedAtMs).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
    return <article className="card" key={entry.id}>
      <div className="task-heading"><BotAvatar seed={botIdentity(entry.toolId)} size={48} /><h3>{entry.toolName}</h3></div>
      {entry.status === "failed" ? <>
        <p>{friendlyFailure(entry.body)}</p>
        <details><summary>{statusWords.failed} · {time}</summary><p>{entry.note}</p><pre>{entry.body}</pre></details>
        <div className="actions"><button onClick={() => run(entry.toolId)}>Try again</button><button onClick={create}>Change the request</button></div>
      </> : <>
        <p>{statusWords[entry.status]} · {time}</p>
        {entry.note && <p>{entry.note}</p>}
        <div className="result-answer preserve-lines">{entry.body}</div>
      </>}
      {entry.fileCreated && <p>Created <FileText text={entry.fileCreated} /></p>}
      {entry.status !== "failed" && <UsedLine parts={usedParts(program, program?.modelStep ? route : null)} />}
      {entry.status === "needs_you" && entry.staged ? <>
        {(entry.staged.plannedCalls ?? []).map((action, i) => <article key={i} aria-label={`Planned action ${i + 1}`}><ActionReview action={action} /></article>)}
        <div className="actions"><button className="primary" disabled={busy} onClick={() => void act(async () => {
          const result = await backend.commitRun(entry.staged!.runId, entry.staged!.planHash, true);
          if ("failure" in result) throw new Error(result.failure.message);
          await refresh();
        })}>Approve and run</button>
        <button disabled={busy} onClick={() => void act(async () => { await backend.discardRun(entry.staged!.runId); await refresh(); })}>Discard</button></div>
      </> : entry.status !== "staged" && <button disabled={busy} onClick={() => void act(async () => { await backend.dismissTodayEntry(entry.id); await refresh(); })}>Dismiss</button>}
    </article>;
  };
  return <section>
    {error && <FailureNotice message={error} retry={() => void act(refresh)} change={tools} />}
    {days === null ? <BotStatus label="Loading Today…" /> : !days.some(d => d.entries.length) ? <div className="empty-state">
      <h2>Scheduled results show up here</h2>
      <div className="actions"><button className="primary" onClick={tools}>Schedule a tool</button><button className="quiet" onClick={create}>Describe a task</button></div>
    </div> : <>
      {!!needs.length && <section><h2>Needs you</h2>{needs.map(show)}</section>}
      {needs.some(e => e.staged?.plannedCalls?.length) && <details><summary>Details</summary>{needs.map(e => <div key={e.id}><p>{e.toolName}</p><ActionTechnicalDetails actions={e.staged?.plannedCalls ?? []} /></div>)}</details>}
      {days.map(day => <section key={day.day}><h2>{day.day === new Date().toLocaleDateString("en-CA") ? "Today's results" : day.day}</h2>{day.entries.filter(e => e.status !== "needs_you").map(show)}</section>)}
    </>}
  </section>;
}
