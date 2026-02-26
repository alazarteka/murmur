import { invoke } from '@tauri-apps/api/core';
import type { AppStatus, AudioInputStatus, HistoryEntry, ModelInfo } from './types';

const bridgeMissingError =
  'Tauri bridge unavailable. Use the Murmur app window from the tray (not a standalone browser tab).';

const safeInvoke = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    const message = String(error);
    if (message.includes('__TAURI_INTERNALS__')) {
      throw new Error(bridgeMissingError);
    }
    throw error;
  }
};

export const startRecording = (): Promise<void> => safeInvoke('start_recording');
export const stopRecording = (): Promise<void> => safeInvoke('stop_recording');
export const toggleRecording = (): Promise<void> => safeInvoke('toggle_recording');
export const cancelTranscription = (): Promise<boolean> => safeInvoke('cancel_transcription');

export const getHistory = (limit = 15): Promise<HistoryEntry[]> =>
  safeInvoke('get_history', { limit });

export const getAppState = (): Promise<AppStatus> => safeInvoke('get_app_state');

export const copyText = (text: string): Promise<void> => safeInvoke('copy_text', { text });

export const deleteTranscription = (id: number): Promise<void> =>
  safeInvoke('delete_transcription', { id });

export const listModels = (): Promise<ModelInfo[]> => safeInvoke('list_models');

export const setActiveModel = (fileName: string): Promise<void> =>
  safeInvoke('set_active_model', { fileName });

export const getHotkey = (): Promise<string> => safeInvoke('get_hotkey');

export const setHotkey = (hotkey: string): Promise<string> => safeInvoke('set_hotkey', { hotkey });

export const getAutoCopy = (): Promise<boolean> => safeInvoke('get_auto_copy');

export const setAutoCopy = (enabled: boolean): Promise<boolean> =>
  safeInvoke('set_auto_copy', { enabled });

export const getAudioInputStatus = (): Promise<AudioInputStatus> =>
  safeInvoke('get_audio_input_status');
