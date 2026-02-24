export type AppStatus = 'idle' | 'recording' | 'processing';

export interface HistoryEntry {
  id: number;
  text: string;
  created_at: string;
  duration_ms: number | null;
  model: string;
}

export interface TranscriptionCompletePayload {
  id: number;
  text: string;
  duration_ms: number;
  model: string;
}

export interface ErrorPayload {
  message: string;
}
