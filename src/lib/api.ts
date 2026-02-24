import { invoke } from '@tauri-apps/api/core';
import type { AppStatus, HistoryEntry, ModelInfo } from './types';

export const startRecording = (): Promise<void> => invoke('start_recording');
export const stopRecording = (): Promise<void> => invoke('stop_recording');
export const toggleRecording = (): Promise<void> => invoke('toggle_recording');

export const getHistory = (limit = 15): Promise<HistoryEntry[]> =>
  invoke('get_history', { limit });

export const getAppState = (): Promise<AppStatus> => invoke('get_app_state');

export const copyText = (text: string): Promise<void> => invoke('copy_text', { text });

export const deleteTranscription = (id: number): Promise<void> =>
  invoke('delete_transcription', { id });

export const listModels = (): Promise<ModelInfo[]> => invoke('list_models');

export const setActiveModel = (fileName: string): Promise<void> =>
  invoke('set_active_model', { file_name: fileName });
