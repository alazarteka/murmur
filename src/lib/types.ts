export type AppStatus = 'idle' | 'recording' | 'processing' | 'cancelling';

export interface HistoryEntry {
  id: number;
  text: string;
  created_at: string;
  duration_ms: number | null;
  model: string;
}

export interface ModelInfo {
  file_name: string;
  label: string;
  quality: string;
  installed: boolean;
  active: boolean;
  download_url: string | null;
}

export interface TranscriptionCompletePayload {
  id: number;
  text: string;
  duration_ms: number;
  model: string;
  auto_copied: boolean;
}

export interface ErrorPayload {
  message: string;
}

export interface NoticePayload {
  message: string;
}

export interface AudioInputStatus {
  available_inputs: number;
  default_input: string | null;
  default_sample_rate: number | null;
  ok: boolean;
  message: string | null;
}
