<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import {
    copyText,
    deleteTranscription,
    getAudioInputStatus,
    getAppState,
    getAutoCopy,
    getHotkey,
    getHistory,
    listModels,
    setAutoCopy,
    setActiveModel,
    setHotkey,
    toggleRecording
  } from './lib/api';
  import type {
    AppStatus,
    AudioInputStatus,
    ErrorPayload,
    HistoryEntry,
    ModelInfo,
    NoticePayload,
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
  let hotkey = 'control+shift+KeyS';
  let hotkeyBusy = false;
  let rebindingHotkey = false;
  let hotkeyMessage = '';
  let hotkeyCaptureCleanup: (() => void) | null = null;
  let autoCopy = false;
  let autoCopyBusy = false;
  let noticeMessage = '';
  let audioStatus: AudioInputStatus | null = null;
  let noticeTimer: number | null = null;

  $: displayModels = models.length > 0 ? models : FALLBACK_MODELS;
  $: activeModelInfo = displayModels.find((model) => model.file_name === activeModel) ?? null;
  $: hotkeyTokens = formatHotkeyTokens(hotkey);

  const modelShortName = (fileName: string): string =>
    fileName.replace(/^ggml-/, '').replace(/\.bin$/, '');

  const formatHotkeyTokens = (shortcut: string): string[] => {
    const tokens = shortcut.split('+').map((part) => part.trim()).filter(Boolean);
    return tokens.map((token) => {
      const normalized = token.toLowerCase();
      if (normalized === 'control' || normalized === 'ctrl') return '⌃';
      if (normalized === 'shift') return '⇧';
      if (normalized === 'alt' || normalized === 'option') return '⌥';
      if (normalized === 'super' || normalized === 'command' || normalized === 'cmd') return '⌘';
      if (normalized.startsWith('key') && token.length === 4) return token[3]?.toUpperCase() ?? token;
      if (normalized.startsWith('digit') && token.length === 6) return token[5] ?? token;
      return token;
    });
  };

  const hotkeyFromEvent = (event: KeyboardEvent): string | null => {
    const ignored = new Set([
      'ControlLeft',
      'ControlRight',
      'ShiftLeft',
      'ShiftRight',
      'AltLeft',
      'AltRight',
      'MetaLeft',
      'MetaRight'
    ]);

    if (ignored.has(event.code)) return null;

    let keyToken: string | null = null;
    if (event.code.startsWith('Key') || event.code.startsWith('Digit')) {
      keyToken = event.code;
    } else if (/^F\d{1,2}$/.test(event.code)) {
      keyToken = event.code;
    } else {
      const mapped: Record<string, string> = {
        Space: 'Space',
        Minus: 'Minus',
        Equal: 'Equal',
        Comma: 'Comma',
        Period: 'Period',
        Semicolon: 'Semicolon',
        Quote: 'Quote',
        Slash: 'Slash',
        Backslash: 'Backslash',
        Backquote: 'Backquote',
        BracketLeft: 'BracketLeft',
        BracketRight: 'BracketRight'
      };
      keyToken = mapped[event.code] ?? null;
    }

    if (!keyToken) return null;

    const parts: string[] = [];
    if (event.ctrlKey) parts.push('control');
    if (event.shiftKey) parts.push('shift');
    if (event.altKey) parts.push('alt');
    if (event.metaKey) parts.push('super');
    if (parts.length === 0) return null;

    parts.push(keyToken);
    return parts.join('+');
  };

  const stopHotkeyCapture = () => {
    if (hotkeyCaptureCleanup) {
      hotkeyCaptureCleanup();
      hotkeyCaptureCleanup = null;
    }
    rebindingHotkey = false;
  };

  const setNotice = (message: string) => {
    noticeMessage = message;
    if (noticeTimer !== null) {
      window.clearTimeout(noticeTimer);
      noticeTimer = null;
    }
    noticeTimer = window.setTimeout(() => {
      noticeMessage = '';
      noticeTimer = null;
    }, 5000);
  };

  const onAutoCopyToggle = async (event: Event) => {
    const target = event.currentTarget as HTMLInputElement | null;
    if (!target) return;

    const next = target.checked;
    autoCopyBusy = true;
    errorMessage = '';
    try {
      autoCopy = await setAutoCopy(next);
    } catch (error) {
      autoCopy = !next;
      errorMessage = `Auto-copy update failed: ${String(error)}`;
    } finally {
      autoCopyBusy = false;
    }
  };

  const startHotkeyCapture = () => {
    if (rebindingHotkey || hotkeyBusy) return;

    hotkeyMessage = '';
    rebindingHotkey = true;

    const onKeyDown = async (event: KeyboardEvent) => {
      if (!rebindingHotkey || event.repeat) return;

      event.preventDefault();
      event.stopPropagation();

      if (event.key === 'Escape') {
        stopHotkeyCapture();
        return;
      }

      const nextHotkey = hotkeyFromEvent(event);
      if (!nextHotkey) {
        hotkeyMessage = 'Press one non-modifier key with Ctrl/Shift/Alt/Cmd.';
        return;
      }

      hotkeyBusy = true;
      try {
        hotkey = await setHotkey(nextHotkey);
        hotkeyMessage = '';
      } catch (error) {
        hotkeyMessage = `Hotkey update failed: ${String(error)}`;
      } finally {
        hotkeyBusy = false;
        stopHotkeyCapture();
      }
    };

    const onBlur = () => {
      stopHotkeyCapture();
    };

    window.addEventListener('keydown', onKeyDown, true);
    window.addEventListener('blur', onBlur, true);

    hotkeyCaptureCleanup = () => {
      window.removeEventListener('keydown', onKeyDown, true);
      window.removeEventListener('blur', onBlur, true);
    };
  };

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
        const [nextStatus, nextHotkey, nextAutoCopy, nextAudioStatus] = await Promise.all([
          getAppState(),
          getHotkey(),
          getAutoCopy(),
          getAudioInputStatus()
        ]);
        status = nextStatus;
        hotkey = nextHotkey;
        autoCopy = nextAutoCopy;
        audioStatus = nextAudioStatus;
        if (!nextAudioStatus.ok && nextAudioStatus.message) {
          setNotice(nextAudioStatus.message);
        }
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
          copiedState = event.payload.auto_copied ? 'Copied' : '';
          await refreshHistory();
          if (event.payload.auto_copied) {
            window.setTimeout(() => {
              copiedState = '';
            }, 1200);
          }
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

      const unlistenHotkeyUpdated = await listen<{ hotkey: string }>('hotkey-updated', (event) => {
        hotkey = event.payload.hotkey;
      });

      const unlistenAutoCopyUpdated = await listen<{ auto_copy: boolean }>(
        'auto-copy-updated',
        (event) => {
          autoCopy = event.payload.auto_copy;
        }
      );

      const unlistenNotice = await listen<NoticePayload>('app-notice', (event) => {
        setNotice(event.payload.message);
      });

      cleanup = () => {
        stopHotkeyCapture();
        if (noticeTimer !== null) {
          window.clearTimeout(noticeTimer);
          noticeTimer = null;
        }
        unlistenStarted();
        unlistenStopped();
        unlistenCompleted();
        unlistenError();
        unlistenModelProgress();
        unlistenModelComplete();
        unlistenHotkeyUpdated();
        unlistenAutoCopyUpdated();
        unlistenNotice();
      };
    };

    void setup();
    return () => cleanup();
  });
</script>

<main class="window">

  <!-- ── Toolbar ─────────────────────────────────── -->
  <header class="toolbar">
    <div class="toolbar-info">
      <h1 class="app-name">Murmur</h1>
      <p class="app-sub">Local speech-to-text</p>
    </div>
    <span class={`status-badge ${status}`}>
      <span class="status-dot"></span>
      {statusLabel(status)}
    </span>
  </header>

  <div class="sep"></div>

  <!-- ── Content ─────────────────────────────────── -->
  <div class="content">

    <!-- Record -->
    <div class="record-section">
      <button
        class={`record-btn${status === 'recording' ? ' is-recording' : ''}${status === 'processing' ? ' is-processing' : ''}`}
        on:click={onToggle}
        disabled={busy || modelBusy || status === 'processing'}
      >
        <span class="btn-dot"></span>
        {#if status === 'recording'}
          Stop Recording
        {:else if status === 'processing'}
          Transcribing…
        {:else}
          Start Recording
        {/if}
      </button>

      <div class="hotkey-row">
        {#each hotkeyTokens as token}
          <span class="kbd">{token}</span>
        {/each}
        <button
          class="btn-inline hotkey-change"
          on:click={startHotkeyCapture}
          disabled={hotkeyBusy || rebindingHotkey || status !== 'idle'}
        >
          {rebindingHotkey ? 'Press keys…' : 'Change'}
        </button>
      </div>
      {#if hotkeyMessage}
        <p class="hotkey-error">{hotkeyMessage}</p>
      {:else if rebindingHotkey}
        <p class="hotkey-hint">Press your new shortcut, or Esc to cancel.</p>
      {/if}

      <div class="auto-copy-row">
        <label class="checkbox-row">
          <input
            type="checkbox"
            checked={autoCopy}
            on:change={onAutoCopyToggle}
            disabled={autoCopyBusy}
          />
          <span>Auto-copy transcripts</span>
        </label>
      </div>
      {#if audioStatus?.default_input}
        <p class="audio-hint">
          Mic: {audioStatus.default_input}
          {#if audioStatus.default_sample_rate}
            · {audioStatus.default_sample_rate} Hz
          {/if}
        </p>
      {/if}
    </div>

    <div class="sep"></div>

    <!-- Model -->
    <div class="form-section">
      <label class="field-label" for="model-select">Model</label>
      <div class="select-wrap">
        <select
          id="model-select"
          value={activeModel}
          on:change={onModelChange}
          disabled={modelBusy}
        >
          {#each displayModels as model}
            <option value={model.file_name}>
              {model.label} — {model.quality}{model.installed ? '' : ' ↓'}
            </option>
          {/each}
        </select>
        <span class="chevron">▾</span>
      </div>
      {#if activeModelInfo}
        <p class="model-caption">
          {activeModelInfo.label} · {activeModelInfo.quality}{#if !activeModelInfo.installed} · <em>not installed</em>{/if}
        </p>
      {/if}
    </div>

    <div class="sep"></div>

    <!-- Download progress -->
    {#if downloadPercent !== null}
      <div class="notice-band info">
        <div class="notice-row">
          <span>Downloading {modelShortName(downloadingModel)}</span>
          <strong>{downloadPercent}%</strong>
        </div>
        <progress max="100" value={downloadPercent}></progress>
      </div>
      <div class="sep"></div>
    {/if}

    {#if noticeMessage}
      <div class="notice-band info">{noticeMessage}</div>
      <div class="sep"></div>
    {/if}

    <!-- Error -->
    {#if errorMessage}
      <div class="notice-band error">{errorMessage}</div>
      <div class="sep"></div>
    {/if}

    <!-- Result -->
    <div class="result-section">
      <div class="section-header">
        <span class="section-label">Result</span>
        <span class={`copied-tag${copiedState ? ' show' : ''}`}>✓ Copied</span>
      </div>
      <textarea bind:value={resultText} placeholder="Transcribed text appears here…"></textarea>
      <div class="action-row">
        <button class="btn-primary" on:click={onCopy} disabled={!resultText.trim()}>Copy</button>
        <button class="btn-secondary" on:click={onDiscard}>Discard</button>
      </div>
    </div>

    <div class="sep"></div>

    <!-- History -->
    <div class="history-section">
      <div class="section-header">
        <span class="section-label">Recent</span>
        <button class="btn-inline" on:click={refreshHistory}>Refresh</button>
      </div>

      {#if history.length === 0}
        <p class="empty-msg">No transcriptions yet.</p>
      {:else}
        <div class="history-list">
          {#each history as item}
            <div class="history-item">
              <button class="history-text-btn" on:click={() => onUseHistoryItem(item.text)}>
                {item.text.slice(0, 180)}
              </button>
              <div class="history-meta-row">
                <div class="history-meta-info">
                  <span>{formatTimestamp(item.created_at)}</span>
                  <span class="dot-sep"></span>
                  <span>{modelShortName(item.model)}</span>
                </div>
                <button class="btn-del" on:click={() => onDelete(item.id)}>✕</button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

  </div>
</main>
