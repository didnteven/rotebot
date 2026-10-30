/** Google's "Download JSON" for an OAuth client: {"installed": {...}} (desktop) or {"web": {...}}. */
export function parseGoogleClientFile(text: string): { clientId: string; clientSecret: string | null } | null {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return null;
  }
  const root = value as Record<string, unknown> | null;
  const inner = (root?.installed ?? root?.web) as Record<string, unknown> | undefined;
  const id = inner?.client_id;
  const secret = inner?.client_secret;
  if (typeof id !== "string" || !id.endsWith(".apps.googleusercontent.com")) return null;
  return { clientId: id, clientSecret: typeof secret === "string" && secret ? secret : null };
}

/** Google services share one OAuth client; each still needs its own API turned on in Google Cloud. */
export const googleConnectorIds = ["gmail", "google-calendar", "google-drive"];
