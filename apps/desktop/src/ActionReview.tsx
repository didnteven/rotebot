import type { PlannedAction } from "./backend/types";

export function actionWords(value: string) {
  const words = value.replace(/([a-z])([A-Z])/g, "$1 $2").replace(/[_-]+/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

// Keep all external action fields visible, including recipients in arrays and
// nested fields, without making users read the serialized argument object.
function FieldValue({ value }: { value: unknown }) {
  if (Array.isArray(value)) {
    return <ul>{value.map((item, i) => <li key={i}><FieldValue value={item} /></li>)}</ul>;
  }
  if (value && typeof value === "object") {
    return <dl>{Object.entries(value).map(([key, item]) => (
      <div key={key}><dt>{actionWords(key)}</dt><dd><FieldValue value={item} /></dd></div>
    ))}</dl>;
  }
  return <>{value === null ? "None" : String(value)}</>;
}

export function ActionReview({ action }: { action: PlannedAction }) {
  const args = action.call.args;
  const fields = args && typeof args === "object" && !Array.isArray(args) ? Object.entries(args) : [];
  const email = fields.some(([key]) => key.toLowerCase() === "to") &&
    /mail|draft|send|message/i.test(`${action.service} ${action.call.toolName}`);
  const main = email
    ? fields.filter(([key]) => ["to", "subject", "body"].includes(key.toLowerCase()))
    : fields.filter(([, value]) => typeof value === "string").slice(0, 4);
  const extra = fields.filter(([key]) => !main.some(([name]) => name === key));
  const body = email ? fields.find(([key]) => key.toLowerCase() === "body")?.[1] : null;
  return <>
    <h4>{action.service} · {actionWords(action.call.toolName)}</h4>
    {(action.destructiveHint === true || action.readOnlyHint !== true) && (
      <p className="notice notice-warning">This changes data in {action.service}</p>
    )}
    <dl>{main.map(([key, value]) => (
      <div key={key}>
        <dt>{actionWords(key)}</dt>
        <dd style={{ whiteSpace: "pre-wrap" }}>
          <FieldValue value={email && key.toLowerCase() === "body" && typeof value === "string"
            ? value.split("\n").slice(0, 3).join("\n") : value} />
        </dd>
      </div>
    ))}</dl>
    {typeof body === "string" && body.split("\n").length > 3 && (
      <p style={{ whiteSpace: "pre-wrap" }}>{body.split("\n").slice(3).join("\n")}</p>
    )}
    {extra.length > 0 && <dl>{extra.map(([key, value]) => (
      <div key={key}><dt>{actionWords(key)}</dt><dd style={{ whiteSpace: "pre-wrap" }}><FieldValue value={value} /></dd></div>
    ))}</dl>}
  </>;
}

export function ActionTechnicalDetails({ actions }: { actions: PlannedAction[] }) {
  return <>{actions.map((a, i) => <div key={i}>
    <p>{a.service} · {a.call.toolName}</p>
    <pre aria-label={`Exact action arguments ${i + 1}`}>{JSON.stringify(a.call.args, null, 2)}</pre>
    <p>Server’s claims: readOnlyHint = {String(a.readOnlyHint)}; destructiveHint = {String(a.destructiveHint)}</p>
  </div>)}</>;
}
