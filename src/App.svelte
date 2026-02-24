<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import {
    copyText,
    deleteTranscription,
    getAppState,
    getHistory,
    listModels,
    setActiveModel,
    toggleRecording
  } from './lib/api';
  import type {
    AppStatus,
    ErrorPayload,
    HistoryEntry,
    ModelInfo,
    TranscriptionCompletePayload
  } from './lib/types';

  const FALLBACK_MODELS: ModelInfo[] = [
    {
      file_name: 'ggml-large-v3-turbo-q5_0.bin',
      label: 'large-v3-turbo-q5_0',
      quality: 'best balance',
      installed: false,
      active: false,
      download_url: null
    },
    {
      file_name: 'ggml-large-v3-turbo.bin',
      label: 'large-v3-turbo',
      quality: 'highest quality (fast)',
      installed: false,
      active: false,
      download_url: null
    },
    {
      file_name: 'ggml-large-v3.bin',
      label: 'large-v3',
      quality: 'highest quality',
      installed: false,
      active: false,
      download_url: null
    },
    {
      file_name: 'ggml-medium.en.bin',
      label: 'medium.en',
      quality: 'high quality',
      installed: false,
      active: false,
      download_url: null
    },
    {
      file_name: 'ggml-small.en.bin',
      label: 'small.en',
      quality: 'better than base',
      installed: false,
      active: false,
      download_url: null
    },
    {
      file_name: 'ggml-base.en.bin',
      label: 'base.en',
      quality: 'balanced',
      installed: false,
      active: true,
      download_url: null
    },
    {
      file_name: 'ggml-tiny.en.bin',
      label: 'tiny.en',
      quality: 'fastest',
      installed: false,
      active: false,
      download_url: null
    }
  ];

  let status: AppStatus = 'idle';
  let resultText = '';
  let history: HistoryEntry[] = [];
  let models: ModelInfo[] = [...FALLBACK_MODELS];
  let activeModel = 'ggml-base.en.bin';
  let errorMessage = '';
  let busy = false;
  let copiedState = '';
  let modelBusy = false;
  let downloadPercent: number | null = null;
  let downloadingModel = '';

  $: displayModels = models.length > 0 ? models : FALLBACK_MODELS;
  $: activeModelInfo = displayModels.find((model) => model.file_name === activeModel) ?? null;

  const refreshHistory = async () => {
    try {
      history = await getHistory(20);
    } catch (error) {
      errorMessage = `History failed: ${String(error)}`;
    }
  };

  const refreshModels = async () => {
    try {
      const remote = await listModels();
      models = remote.length > 0 ? remote : [...FALLBACK_MODELS];
      const active = models.find((model) => model.active);
      if (active) {
        activeModel = active.file_name;
      }
    } catch (error) {
      models = [...FALLBACK_MODELS];
      activeModel = 'ggml-base.en.bin';
      errorMessage = `Model list failed: ${String(error)}`;
    }
  };

  const onToggle = async () => {
    busy = true;
    errorMessage = '';
    try {
      await toggleRecording();
    } catch (error) {
      errorMessage = String(error);
    } finally {
      busy = false;
    }
  };

  const onCopy = async () => {
    if (!resultText.trim()) return;
    await copyText(resultText);
    copiedState = 'Copied';
    window.setTimeout(() => {
      copiedState = '';
    }, 1200);
  };

  const onDiscard = () => {
    resultText = '';
  };

  const onDelete = async (id: number) => {
    await deleteTranscription(id);
    await refreshHistory();
  };

  const onModelSelect = async (fileName: string) => {
    if (modelBusy) return;
    if (fileName === activeModel && activeModelInfo?.installed) return;

    activeModel = fileName;
    modelBusy = true;
    errorMessage = '';

    try {
      await setActiveModel(fileName);
      await refreshModels();
    } catch (error) {
      errorMessage = String(error);
      await refreshModels();
    } finally {
      modelBusy = false;
      if (downloadPercent === null || downloadPercent >= 100) {
        downloadPercent = null;
        downloadingModel = '';
      }
    }
  };

  const onModelChange = async (event: Event) => {
    const target = event.currentTarget as HTMLSelectElement | null;
    if (!target) return;
    await onModelSelect(target.value);
  };

  const onUseHistoryItem = async (text: string) => {
    resultText = text;
    await copyText(text);
    copiedState = 'Copied';
    window.setTimeout(() => {
      copiedState = '';
    }, 1200);
  };

  const statusLabel = (current: AppStatus): string => {
    if (current === 'recording') return 'Recording';
    if (current === 'processing') return 'Transcribing';
    return 'Idle';
  };

  const formatTimestamp = (value: string): string => {
    const normalized = value.includes('T') ? value : value.replace(' ', 'T');
    const parsed = new Date(normalized);
    if (Number.isNaN(parsed.getTime())) {
      return value;
    }
    return parsed.toLocaleString([], {
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit'
    });
  };

  onMount(() => {
    let cleanup = () => {};

    const setup = async () => {
      try {
        status = await getAppState();
        await Promise.all([refreshHistory(), refreshModels()]);
      } catch (error) {
        errorMessage = String(error);
        return;
      }

      const unlistenStarted = await listen('recording-started', () => {
        status = 'recording';
        errorMessage = '';
      });

      const unlistenStopped = await listen('recording-stopped', () => {
        status = 'processing';
      });

      const unlistenCompleted = await listen<TranscriptionCompletePayload>(
        'transcription-complete',
        async (event) => {
          status = 'idle';
          resultText = event.payload.text;
          copiedState = 'Copied';
          await refreshHistory();
          window.setTimeout(() => {
            copiedState = '';
          }, 1200);
        }
      );

      const unlistenError = await listen<ErrorPayload>('transcription-error', (event) => {
        status = 'idle';
        errorMessage = event.payload.message;
      });

      const unlistenModelProgress = await listen<{ file_name: string; percent: number }>(
        'model-download-progress',
        (event) => {
          modelBusy = true;
          downloadingModel = event.payload.file_name;
          downloadPercent = event.payload.percent;
        }
      );

      const unlistenModelComplete = await listen<{ file_name: string }>(
        'model-download-complete',
        async (event) => {
          modelBusy = false;
          downloadingModel = event.payload.file_name;
          downloadPercent = 100;
          await refreshModels();
          window.setTimeout(() => {
            downloadPercent = null;
            downloadingModel = '';
          }, 1200);
        }
      );

      cleanup = () => {
        unlistenStarted();
        unlistenStopped();
        unlistenCompleted();
        unlistenError();
        unlistenModelProgress();
        unlistenModelComplete();
      };
    };

    void setup();
    return () => cleanup();
  });
</script>

<main class="workspace">
  <header class="panel-header">
    <div>
      <h1>Murmur</h1>
      <p>Use <strong>Ctrl+Shift+S</strong> to start and stop recording.</p>
    </div>
    <span class={`status-pill ${status}`}>{statusLabel(status)}</span>
  </header>

  <section class="card controls">
    <button class="record-button" on:click={onToggle} disabled={busy || modelBusy}>
      {status === 'recording' ? 'Stop Recording' : 'Start Recording'}
    </button>

    <label class="model-field" for="model-select">
      <span>Model</span>
      <select
        id="model-select"
        value={activeModel}
        on:change={onModelChange}
        disabled={modelBusy}
      >
        {#each displayModels as model}
          <option value={model.file_name}>
            {model.label} - {model.quality}{model.installed ? '' : ' (download)'}
          </option>
        {/each}
      </select>
    </label>
  </section>

  {#if downloadPercent !== null}
    <section class="notice">
      <div class="notice-row">
        <span>Downloading {downloadingModel}</span>
        <strong>{downloadPercent}%</strong>
      </div>
      <progress max="100" value={downloadPercent}></progress>
    </section>
  {/if}

  {#if errorMessage}
    <section class="error-banner">{errorMessage}</section>
  {/if}

  <section class="card result-card">
    <div class="row">
      <h2>Result</h2>
      <span class="chip">{copiedState || 'Clipboard ready'}</span>
    </div>
    <textarea bind:value={resultText} placeholder="Transcribed text appears here"></textarea>
    <div class="actions">
      <button on:click={onCopy} disabled={!resultText.trim()}>Copy</button>
      <button class="ghost" on:click={onDiscard}>Discard</button>
    </div>
  </section>

  <section class="card history-card">
    <div class="row">
      <h2>Recent</h2>
      <button class="ghost" on:click={refreshHistory}>Refresh</button>
    </div>

    {#if history.length === 0}
      <p class="empty">No transcriptions yet.</p>
    {:else}
      <div class="history-list">
        {#each history as item}
          <article class="history-item">
            <button class="history-text" on:click={() => onUseHistoryItem(item.text)}>
              {item.text.slice(0, 180)}
            </button>
            <div class="history-meta">
              <span>{formatTimestamp(item.created_at)}</span>
              <span>{item.model}</span>
              <button class="ghost danger" on:click={() => onDelete(item.id)}>Delete</button>
            </div>
          </article>
        {/each}
      </div>
    {/if}
  </section>
</main>
