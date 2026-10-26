import type { AppInfo, Backend, ToolSummary } from './types';

/** A fresh instance has isolated state. It never persists browser data. */
export class MockBackend implements Backend {
  private readonly tools: ToolSummary[];

  constructor(tools: ToolSummary[] = []) {
    this.tools = tools.map((tool) => ({ ...tool }));
  }

  async appInfo(): Promise<AppInfo> {
    return { version: '0.1.0', platform: 'browser' };
  }

  async listTools(): Promise<ToolSummary[]> {
    return this.tools.map((tool) => ({ ...tool }));
  }
}
