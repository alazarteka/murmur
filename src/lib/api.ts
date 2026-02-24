import { invoke } from '@tauri-apps/api/core';
import type { AppStatus, HistoryEntry, ModelInfo } from './types';

const bridgeMissingError =
  'Tauri bridge unavailable. Use the Murmur app window from the tray (not a standalone browser tab).';

const safeInvoke = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
  const win = window as Window & {
    __TAURI__?: { core?: { invoke?: <R>(cmd: string, payload?: Record<string, unknown>) => Promise<R> } };
    __TAURI_INTERNALS__?: { invoke?: <R>(cmd: string, payload?: Record<string, unknown>) => Promise<R> };
  };

  const globalInvoke = win.__TAURI__?.core?.invoke;
  if (typeof globalInvoke === 'function') {
    return globalInvoke<T>(command, args);
  }

  if (typeof win.__TAURI_INTERNALS__?.invoke === 'function') {
    return invoke<T>(command, args);
  }

  throw new Error(bridgeMissingError);
};

export const startRecording = (): Promise<void> => safeInvoke('start_recording');
export const stopRecording = (): Promise<void> => safeInvoke('stop_recording');
export const toggleRecording = (): Promise<void> => safeInvoke('toggle_recording');

export const getHistory = (limit = 15): Promise<HistoryEntry[]> =>
  safeInvoke('get_history', { limit });

export const getAppState = (): Promise<AppStatus> => safeInvoke('get_app_state');

export const copyText = (text: string): Promise<void> => safeInvoke('copy_text', { text });

export const deleteTranscription = (id: number): Promise<void> =>
  safeInvoke('delete_transcription', { id });

export const listModels = (): Promise<ModelInfo[]> => safeInvoke('list_models');

export const setActiveModel = (fileName: string): Promise<void> =>
  safeInvoke('set_active_model', { fileName });
