import { afterEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { createBackend } from './index';
import { MockBackend } from './mock';
import { TauriBackend } from './tauri';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

afterEach(() => { Reflect.deleteProperty(window, '__TAURI_INTERNALS__'); });

describe('typed backend seam', () => {
  it('selects the memory backend in a browser', async () => {
    const backend = createBackend();
    expect(backend).toBeInstanceOf(MockBackend);
    expect(await backend.appInfo()).toEqual({ version: '0.1.0', platform: 'browser' });
    expect(await backend.listTools()).toEqual([]);
    expect(invoke).not.toHaveBeenCalled();
  });

  it('selects native IPC in Tauri and maps both command names', async () => {
    Object.defineProperty(window, '__TAURI_INTERNALS__', { value: {}, configurable: true });
    const backend = createBackend();
    expect(backend).toBeInstanceOf(TauriBackend);
    vi.mocked(invoke).mockResolvedValueOnce({ version: '0.1.0', platform: 'macos' }).mockResolvedValueOnce([]);
    expect(await backend.appInfo()).toEqual({ version: '0.1.0', platform: 'macos' });
    expect(await backend.listTools()).toEqual([]);
    expect(invoke).toHaveBeenNthCalledWith(1, 'app_info');
    expect(invoke).toHaveBeenNthCalledWith(2, 'list_tools');
  });

  it('isolates memory state from constructor inputs and returned objects', async () => {
    const tools = [{ id: 'tool-1', name: 'Orders', version: 1 }];
    const backend = new MockBackend(tools);
    tools[0].name = 'changed';
    const result = await backend.listTools();
    result[0].name = 'also changed';
    expect((await backend.listTools())[0].name).toBe('Orders');
    expect(await new MockBackend().listTools()).toEqual([]);
  });
});
