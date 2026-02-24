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

  let status: AppStatus = 'idle';
  let resultText = '';
  let history: HistoryEntry[] = [];
  let models: ModelInfo[] = [];
  let activeModel = '';
  let errorMessage = '';
  let busy = false;
  let copiedState = '';
  let modelBusy = false;
  let downloadPercent: number | null = null;
  let downloadingModel = '';

  const refreshHistory = async () => {
    history = await getHistory(20);
  };

  const refreshModels = async () => {
    models = await listModels();
    const active = models.find((model) => model.active);
    if (active) {
      activeModel = active.file_name;
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

  const onModelChange = async () => {
    if (!activeModel) return;

    modelBusy = true;
    errorMessage = '';
    try {
      await setActiveModel(activeModel);
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
    return value.replace('T', ' ');
  };

  onMount(() => {
    let cleanup = () => {};

    const setup = async () => {
      status = await getAppState();
      await Promise.all([refreshHistory(), refreshModels()]);

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
  <header class="hero">
    <div>
      <h1>Murmur</h1>
      <p>Press <strong>Ctrl+Shift+S</strong> to toggle recording. The app stays in your tray.</p>
    </div>
    <span class={`status-pill ${status}`}>{statusLabel(status)}</span>
  </header>

  <section class="command-bar">
    <button class="record-button" on:click={onToggle} disabled={busy}>
      {status === 'recording' ? 'Stop Recording' : 'Start Recording'}
    </button>

    <label class="model-picker">
      <span>Model</span>
      <select bind:value={activeModel} on:change={onModelChange} disabled={modelBusy}>
        {#each models as model}
          <option value={model.file_name}>
            {model.label} · {model.quality}{model.installed ? '' : ' (not installed)'}
          </option>
        {/each}
      </select>
    </label>
  </section>

  {#if downloadPercent !== null}
    <section class="notice">
      Downloading <strong>{downloadingModel}</strong>: {downloadPercent}%
    </section>
  {/if}

  {#if errorMessage}
    <section class="error-banner">{errorMessage}</section>
  {/if}

  <section class="content-grid">
    <article class="card result-card">
      <div class="row">
        <h2>Result</h2>
        <span class="chip">{copiedState || 'Clipboard ready'}</span>
      </div>
      <textarea bind:value={resultText} placeholder="Transcribed text appears here"></textarea>
      <div class="actions">
        <button on:click={onCopy} disabled={!resultText.trim()}>Copy</button>
        <button class="ghost" on:click={onDiscard}>Discard</button>
      </div>
    </article>

    <article class="card history-card">
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
    </article>
  </section>
</main>
