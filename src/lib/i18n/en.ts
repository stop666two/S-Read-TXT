// English catalog. Key set must exactly match zh-CN（由 `Record<MessageKey, string>` 类型强制）.

import type { MessageKey } from './zh-CN';

export const en: Record<MessageKey, string> = {
  // ---------- Title bar ----------
  'titlebar.settings': 'Open settings',
  'titlebar.minimize': 'Minimize',
  'titlebar.maximize': 'Maximize',
  'titlebar.restore': 'Restore',
  'titlebar.close': 'Close',

  // ---------- Toolbar ----------
  'toolbar.open': 'Open file',
  'toolbar.openHint': 'Open file (Ctrl+O)',
  'toolbar.history': 'History',
  'toolbar.historyHint': 'History (Ctrl+Shift+H)',
  'toolbar.toggleEdit': 'Toggle edit mode',
  'toolbar.editEnableHint': 'Enable edit mode (Ctrl+E)',
  'toolbar.editDisableHint': 'Exit edit mode (Ctrl+E)',
  'toolbar.editReadOnlyHint':
    'File exceeds the read-only threshold and cannot be edited (adjustable in settings)',
  'toolbar.save': 'Save',
  'toolbar.saveHint': 'Save (Ctrl+S)',
  'toolbar.encoding': 'Encoding: {value}',
  'toolbar.encodingAuto': 'Auto',
  'toolbar.theme': 'Theme: {name} (click to cycle)',
  'toolbar.themeCycle': 'Cycle theme',
  'toolbar.settings': 'Settings',

  // ---------- Theme names ----------
  'theme.light': 'Light',
  'theme.dark': 'Dark',
  'theme.eye': 'Eye care',
  'theme.system': 'Follow system',

  // ---------- Status bar ----------
  'status.reading': 'Read {percent}%',
  'status.readOnly': 'Read-only',
  'status.readOnlyHint':
    'File exceeds the read-only threshold and cannot be edited (adjustable in settings)',
  'status.encodingHint': 'Switch encoding',
  'status.noFile': 'No file open',

  // ---------- Empty state ----------
  'empty.title': 'No file open',
  'empty.hint': 'Drag a TXT file into the window, or click the button below',
  'empty.open': 'Open file',
  'empty.shortcutHint': 'Shortcut: Ctrl+O · Multiple files supported',

  // ---------- Drop overlay ----------
  'drop.release': 'Release to open the TXT file',

  // ---------- Onboarding ----------
  'onboarding.aria': 'Getting started',
  'onboarding.title': 'Welcome to S-Read-TXT',
  'onboarding.openFile': 'Open files',
  'onboarding.openFileDesc':
    ': use the toolbar "Open" button or drag a TXT file into the window (Ctrl+O)',
  'onboarding.tabs': 'Tabs',
  'onboarding.tabsDesc': ': Ctrl+W closes, Ctrl+Tab switches, Ctrl+1~9 jumps to the Nth tab',
  'onboarding.history': 'History',
  'onboarding.historyDesc':
    ': opened files are recorded automatically; manage them under Settings → History',
  'onboarding.shortcuts': 'Shortcuts',
  'onboarding.shortcutsDesc':
    ': Settings → Shortcuts; all 15 actions are customizable',
  'onboarding.dontShow': "Don't show again",
  'onboarding.start': 'Get started',
};
