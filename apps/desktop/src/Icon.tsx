/** Decorative, bundled icons. Labels stay in text beside them. */
export type IconName = "today" | "tools" | "activity" | "connections" | "settings" | "mail" | "files" | "globe" | "shield";
export function Icon({ name }: { name: IconName }) {
  return <span className={`ui-icon icon-${name}`} aria-hidden="true" />;
}
/** The icon for a connected service, shared by Connections and the create flow. */
export function serviceIcon(connector: { id: string; registration?: string; mail?: unknown }): IconName {
  return connector.registration === "api" ? "globe"
    : connector.mail || /gmail|mail|outlook/.test(connector.id) ? "mail"
    : /calendar/.test(connector.id) ? "today"
    : /drive|notion/.test(connector.id) ? "files" : "connections";
}
