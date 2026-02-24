<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import {
    copyText,
    deleteTranscription,
    getAppState,
    getHistory,
    toggleRecording
  } from './lib/api';
  import type {
    AppStatus,
    ErrorPayload,
    HistoryEntry,
    TranscriptionCompletePayload
  } from './lib/types';

  let status: AppStatus = 'idle';
  let resultText = '';
  let history: HistoryEntry[] = [];
  let errorMessage = '';
  let busy = false;

  const refreshHistory = async () => {
    history = await getHistory(15);
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
  };

  const onDiscard = () => {
    resultText = '';
  };

  const onDelete = async (id: number) => {
    await deleteTranscription(id);
    await refreshHistory();
  };

  const statusLabel = (current: AppStatus): string => {
    if (current === 'recording') return 'Recording';
    if (current === 'processing') return 'Transcribing';
    return 'Idle';
  };

  onMount(() => {
    let cleanup = () => {};

    const setup = async () => {
      status = await getAppState();
      await refreshHistory();

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
          await copyText(resultText);
          await refreshHistory();
        }
      );

      const unlistenError = await listen<ErrorPayload>('transcription-error', (event) => {
        status = 'idle';
        errorMessage = event.payload.message;
      });

      cleanup = () => {
        unlistenStarted();
        unlistenStopped();
        unlistenCompleted();
        unlistenError();
      };
    };

    void setup();
    return () => cleanup();
  });
</script>

<main class="panel">
  <section class="card">
    <div class="row">
      <strong>Press Ctrl+Shift+S</strong>
      <span class={`status-dot ${status}`}></span>
    </div>
    <p>{statusLabel(status)}</p>
    <div class="row">
      <button on:click={onToggle} disabled={busy}>
        {status === 'recording' ? 'Stop' : 'Record'}
      </button>
    </div>
  </section>

  {#if errorMessage}
    <section class="card error">{errorMessage}</section>
  {/if}

  <section class="card">
    <div class="row">
      <strong>Result</strong>
      <small>Editable</small>
    </div>
    <textarea bind:value={resultText} placeholder="Transcribed text appears here"></textarea>
    <div class="row">
      <button on:click={onCopy} disabled={!resultText.trim()}>Copy</button>
      <button class="secondary" on:click={onDiscard}>Discard</button>
    </div>
  </section>

  <section class="card history">
    <div class="row">
      <strong>Recent</strong>
      <button class="secondary" on:click={refreshHistory}>Refresh</button>
    </div>
    {#if history.length === 0}
      <p class="meta">No transcriptions yet.</p>
    {:else}
      {#each history as item}
        <article class="history-item">
          <div class="preview">{item.text.slice(0, 160)}</div>
          <div class="row meta">
            <span>{item.created_at}</span>
            <button class="secondary" on:click={() => onDelete(item.id)}>Delete</button>
          </div>
        </article>
      {/each}
    {/if}
  </section>
</main>
