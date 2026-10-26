export interface AppInfo {
  version: string;
  platform: string;
}

export interface ToolSummary {
  id: string;
  name: string;
  version: number;
}

/** Product code talks only to this seam, in native and browser environments. */
export interface Backend {
  appInfo(): Promise<AppInfo>;
  listTools(): Promise<ToolSummary[]>;
}
