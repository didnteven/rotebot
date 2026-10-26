import { invoke } from '@tauri-apps/api/core';
import type { AppInfo, Backend, ToolSummary } from './types';

export class TauriBackend implements Backend {
  appInfo(): Promise<AppInfo> {
    return invoke<AppInfo>('app_info');
  }

  listTools(): Promise<ToolSummary[]> {
    return invoke<ToolSummary[]>('list_tools');
  }
}
