import { MockBackend } from './mock';
import { TauriBackend } from './tauri';
import type { Backend } from './types';

export function createBackend(): Backend {
  return '__TAURI_INTERNALS__' in window ? new TauriBackend() : new MockBackend();
}
