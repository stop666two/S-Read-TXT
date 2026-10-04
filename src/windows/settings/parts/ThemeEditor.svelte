<!--
  ThemeEditor — 自定义主题编辑器（设置自定义化 S3）。
  组成：基于主题载入 → 13 项令牌颜色自由编辑（文本/取色器）→ 命名与 id → 保存为用户主题；
  另含「智能配色」（离线生成）与「AI 生成」占位按钮。
  契约：data-theme-editor 卡片；data-theme-editor-* 动作；input[data-token=<name>] 令牌值。
-->
<script lang="ts">
  import { onMount } from 'svelte';

  import { ask } from '@tauri-apps/plugin-dialog';

  import type { MessageKey } from '../../../lib/i18n/zh-CN';
  import { i18n, t } from '../../../lib/i18n/index.svelte';
  import { describeIpcError, ipc, toIpcError, type ThemeManifest, type ThemeSummary } from '../../../lib/ipc';
  import { toasts } from '../../../lib/state/toasts.svelte';
  import {
    THEME_TOKENS,
    generatePalette,
    suggestThemeName,
    type ThemeToken,
  } from '../../../lib/theme/palette';
  import { settings } from '../store.svelte';

  /** 种子颜色默认值（与内置浅色主题强调色一致）。 */
  const DEFAULT_SEED = '#3B6EA5';

  let sourceId = $state('light');
  let nameZh = $state('');
  let nameEn = $state('');
  let id = $state('');
  let base = $state<'light' | 'dark'>('light');
  let tokens = $state<Record<ThemeToken, string>>(generatePalette(DEFAULT_SEED, 'light'));
  let seed = $state(DEFAULT_SEED);
  let busy = $state(false);
  let errorText = $state('');

  /** 主题本地化名（当前语言） */
  function nameOf(theme: ThemeSummary): string {
    return i18n.locale === 'en' ? theme.nameEn : theme.name;
  }

  /** 生成建议 id（小写字母/数字/连字符，时间戳 base36 保证不重名） */
  function suggestId(): string {
    return `custom-${Date.now().toString(36)}`;
  }

  /** 取色器值（仅接受 6 位十六进制；否则回退黑色，文本值不受影响） */
  function pickerOf(value: string): string {
    return /^#[0-9a-fA-F]{6}$/.test(value.trim()) ? value.trim() : '#000000';
  }

  function setToken(token: ThemeToken, value: string): void {
    tokens = { ...tokens, [token]: value };
  }

  onMount(() => {
    void settings.loadThemes();
  });

  /** 从所选主题载入令牌到表单（内置或用户主题均可） */
  async function loadSource(): Promise<void> {
    try {
      const resolved = await ipc.getTheme(sourceId);
      base = resolved.base === 'dark' ? 'dark' : 'light';
      const next = {} as Record<ThemeToken, string>;
      for (const token of THEME_TOKENS) {
        next[token] = resolved.tokens[token] ?? tokens[token];
      }
      tokens = next;
      errorText = '';
    } catch (error) {
      toasts.error(describeIpcError(toIpcError(error)));
    }
  }

  /** 智能配色：以种子 + 明暗基底离线生成全套令牌 */
  function smartPalette(): void {
    tokens = generatePalette(seed, base);
    const suggested = suggestThemeName(base);
    if (!nameZh.trim() && !nameEn.trim()) {
      nameZh = suggested.name;
      nameEn = suggested.nameEn;
    }
    if (!id.trim()) id = suggestId();
    errorText = '';
  }

  /** AI 生成（占位；后续接入联网服务时替换实现） */
  function aiGenerate(): void {
    toasts.show(t('theme.editor.aiSoon'));
  }

  /** 重置表单（回到默认种子与初始命名） */
  function resetForm(): void {
    nameZh = '';
    nameEn = '';
    id = '';
    base = 'light';
    seed = DEFAULT_SEED;
    tokens = generatePalette(DEFAULT_SEED, 'light');
    errorText = '';
  }

  /** 保存为用户主题（同名用户主题覆盖前确认；错误就地展示） */
  async function save(): Promise<void> {
    errorText = '';
    const trimmedName = nameZh.trim();
    const trimmedNameEn = nameEn.trim();
    const trimmedId = id.trim();
    if (!trimmedName || !trimmedNameEn) {
      errorText = t('theme.editor.needName');
      return;
    }
    if (!/^[a-z0-9-]{1,32}$/.test(trimmedId) || trimmedId === 'system') {
      errorText = t('theme.editor.invalidId');
      return;
    }
    const existing = settings.themes.find((theme) => theme.id === trimmedId);
    if (existing) {
      const confirmed = await ask(t('theme.editor.overwrite', { name: nameOf(existing) }), {
        title: t('theme.editor.title'),
        kind: 'warning',
      });
      if (!confirmed) return;
    }
    const manifest: ThemeManifest = {
      schemaVersion: 1,
      id: trimmedId,
      name: trimmedName,
      nameEn: trimmedNameEn,
      base,
      builtin: false,
      tokens: { ...tokens },
    };
    busy = true;
    try {
      await ipc.saveTheme(manifest);
      await settings.loadThemes();
      toasts.show(t('theme.editor.saved', { name: trimmedName }));
    } catch (error) {
      errorText = describeIpcError(toIpcError(error));
    } finally {
      busy = false;
    }
  }
</script>

<div class="editor-card" data-theme-editor>
  <div class="editor-head">
    <span class="editor-title">{t('theme.editor.title')}</span>
    <small>{t('theme.editor.desc')}</small>
  </div>

  <div class="editor-grid">
    <label class="field">
      <span>{t('theme.editor.source')}</span>
      <select data-theme-editor-source bind:value={sourceId}>
        {#each settings.themes as theme (theme.id)}
          <option value={theme.id}>{nameOf(theme)}</option>
        {/each}
      </select>
    </label>
    <button class="mini" type="button" data-theme-editor-load onclick={() => void loadSource()}>
      {t('theme.editor.load')}
    </button>

    <label class="field">
      <span>{t('theme.editor.nameZh')}</span>
      <input type="text" data-theme-editor-name bind:value={nameZh} spellcheck="false" />
    </label>
    <label class="field">
      <span>{t('theme.editor.nameEn')}</span>
      <input type="text" data-theme-editor-name-en bind:value={nameEn} spellcheck="false" />
    </label>
    <label class="field">
      <span>{t('theme.editor.id')}</span>
      <input type="text" data-theme-editor-id bind:value={id} spellcheck="false" placeholder="custom-…" />
    </label>
    <label class="field">
      <span>{t('theme.editor.base')}</span>
      <select data-theme-editor-base bind:value={base}>
        <option value="light">{t('theme.editor.baseLight')}</option>
        <option value="dark">{t('theme.editor.baseDark')}</option>
      </select>
    </label>
    <label class="field">
      <span>{t('theme.editor.seed')}</span>
      <span class="seed-row">
        <input class="seed-pick" type="color" value={pickerOf(seed)} aria-label={t('theme.editor.seed')} onchange={(event) => (seed = (event.currentTarget as HTMLInputElement).value)} />
        <input type="text" data-theme-editor-seed bind:value={seed} spellcheck="false" />
      </span>
    </label>
    <span class="action-row">
      <button class="btn" type="button" data-theme-editor-palette onclick={smartPalette}>
        {t('theme.editor.palette')}
      </button>
      <button class="mini" type="button" data-theme-editor-ai onclick={aiGenerate}>
        {t('theme.editor.ai')}
      </button>
    </span>
  </div>

  <div class="token-grid">
    {#each THEME_TOKENS as token (token)}
      <label class="token-row">
        <span class="token-name">{t(`theme.token.${token}` as MessageKey)}</span>
        <input
          class="token-pick"
          type="color"
          value={pickerOf(tokens[token])}
          aria-label={t(`theme.token.${token}` as MessageKey)}
          onchange={(event) => setToken(token, (event.currentTarget as HTMLInputElement).value)}
        />
        <input
          class="token-text"
          type="text"
          data-token={token}
          spellcheck="false"
          value={tokens[token]}
          onchange={(event) => setToken(token, (event.currentTarget as HTMLInputElement).value.trim())}
        />
      </label>
    {/each}
  </div>

  {#if errorText}
    <p class="editor-error" data-theme-editor-error>{errorText}</p>
  {/if}

  <div class="editor-actions">
    <button class="btn" type="button" data-theme-editor-save disabled={busy} onclick={() => void save()}>
      {t('theme.editor.save')}
    </button>
    <button class="mini" type="button" data-theme-editor-reset onclick={resetForm}>
      {t('theme.editor.reset')}
    </button>
  </div>
</div>

<style>
  .editor-card {
    margin: 12px 0;
    padding: 12px 14px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--base);
  }

  .editor-head {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 10px;
  }

  .editor-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--ink);
  }

  .editor-head small {
    font-size: 11px;
    color: var(--muted);
  }

  .editor-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px 12px;
    align-items: end;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 12px;
    color: var(--muted);
  }

  .field input[type='text'],
  .field select {
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--surface);
    color: var(--ink);
    font: inherit;
    outline: none;
  }

  .field input[type='text']:focus,
  .field select:focus {
    border-color: var(--accent);
  }

  .seed-row {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .seed-pick {
    width: 30px;
    height: 26px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--surface);
    cursor: pointer;
  }

  .action-row {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .token-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 6px 12px;
    margin-top: 10px;
  }

  .token-row {
    display: grid;
    grid-template-columns: 88px 30px minmax(0, 1fr);
    gap: 6px;
    align-items: center;
    font-size: 12px;
    color: var(--muted);
  }

  .token-pick {
    width: 30px;
    height: 24px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--surface);
    cursor: pointer;
  }

  .token-text {
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--surface);
    color: var(--ink);
    font: inherit;
    font-size: 12px;
    outline: none;
  }

  .token-text:focus {
    border-color: var(--accent);
  }

  .editor-error {
    margin: 8px 0 0;
    font-size: 12px;
    color: var(--danger, #c0392b);
  }

  .editor-actions {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-top: 10px;
  }

  .mini {
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--line);
    border-radius: 5px;
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-size: 12px;
    cursor: default;
  }

  .mini:hover {
    background: var(--hover);
    color: var(--ink);
  }
</style>
