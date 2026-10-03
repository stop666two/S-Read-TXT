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

  // ---------- Common ----------
  'common.cancel': 'Cancel',
  'common.save': 'Save',

  // ---------- Find bar ----------
  'find.aria': 'Find and replace',
  'find.query': 'Find',
  'find.regex': 'Regular expression',
  'find.regexHint':
    'Regular expression (Rust regex syntax; replacements support $1 capture expansion)',
  'find.caseSensitive': 'Match case',
  'find.next': 'Next',
  'find.close': 'Close find',
  'find.closeHint': 'Close (Esc)',
  'find.replacement': 'Replace with',
  'find.replace': 'Replace',
  'find.replaceAll': 'Replace all',

  // ---------- Save dialog ----------
  'save.title': 'Save file',
  'save.targetEncoding': 'Target encoding',
  'save.keepCurrent': 'Keep current encoding ({encoding})',
  'save.backup': 'Write a .bak backup (overwrites the previous on-disk content)',

  // ---------- Unsaved dialog ----------
  'unsaved.discard': "Don't save",

  // ---------- Data folder dialog ----------
  'dataDir.title': 'Data folder not writable',
  'dataDir.line':
    'The program folder is not writable, so history, settings and session cannot be saved:',
  'dataDir.chooseHint1': 'Choose a writable folder (',
  'dataDir.chooseHintStrong': 'effective for this session only',
  'dataDir.chooseHint2':
    '); once the program folder is writable again, the next launch returns to portable mode.',
  'dataDir.busy': 'Working…',
  'dataDir.choose': 'Choose writable folder (recommended)',
  'dataDir.skip': 'Read-only for this session',

  // ---------- Replace-all preview ----------
  'replacePreview.aria': 'Replace-all preview',
  'replacePreview.title': 'Confirm replace all',
  'replacePreview.summarySelected': '{total} matches found, {selected} selected.',
  'replacePreview.summaryTruncated':
    '{total} matches found. Too many to list; only the first {shown} are listed. Unlisted matches will also be replaced.',
  'replacePreview.checkAria': 'Replace row {row}',
  'replacePreview.lineNo': 'Row {row}',
  'replacePreview.replacementLabel': 'Replace with',
  'replacePreview.selectAll': 'Select all',
  'replacePreview.selectNone': 'Select none',
  'replacePreview.confirmAll': 'Replace all {total}',
  'replacePreview.confirmSelected': 'Replace {selected} selected',

  // ---------- History panel ----------
  'history.title': 'History',
  'history.search': 'Search file name or path',
  'history.searchAria': 'Search history',
  'history.close': 'Close history panel',
  'history.empty': 'No history yet',
  'history.noMatch': 'No matching records',
  'history.deleteAria': 'Delete history: {name}',
  'history.deleteHint': 'Delete this record',
  'history.progress': 'Read {percent}%',
  'history.noProgress': 'No progress recorded',
  'history.count': '{filtered} of {total} shown',
  'history.clear': 'Clear history',
  'history.clearTitle': 'Clear history',
  'history.clearMessage': 'This deletes all history and reading progress permanently. Continue?',
  'history.clearConfirm': 'Clear',

  // ---------- Menu bar ----------
  'menu.aria': 'Main menu',
  'menu.file': 'File',
  'menu.edit': 'Edit',
  'menu.view': 'View',
  'menu.help': 'Help',
  'menu.file.open': 'Open file…',
  'menu.file.reload': 'Reload',
  'menu.file.recent': 'Open recent',
  'menu.file.recentAria': 'Open recent',
  'menu.file.history': 'History',
  'menu.file.settings': 'Settings…',
  'menu.file.quit': 'Exit',
  'menu.edit.undo': 'Undo',
  'menu.edit.redo': 'Redo',
  'menu.edit.cut': 'Cut',
  'menu.edit.copy': 'Copy',
  'menu.edit.paste': 'Paste',
  'menu.edit.selectAll': 'Select all',
  'menu.edit.find': 'Find…',
  'menu.edit.replace': 'Replace…',
  'menu.edit.enableEdit': 'Enable edit mode',
  'menu.edit.disableEdit': 'Exit edit mode',
  'menu.edit.save': 'Save',
  'menu.edit.saveAs': 'Save as…',
  'menu.view.fontIncrease': 'Increase font size',
  'menu.view.fontDecrease': 'Decrease font size',
  'menu.view.fontReset': 'Reset font size',
  'menu.view.fullscreen': 'Full screen',
  'menu.help.shortcuts': 'Shortcuts…',
  'menu.help.about': 'About S-Read-TXT',

  // ---------- Tab bar ----------
  'tabBar.aria': 'Open files',
  'tabBar.closeHint': 'Close tab (Ctrl+W)',
  'tabBar.closeAria': 'Close {name}',

  // ---------- Tab context menu ----------
  'tabMenu.aria': 'Tab actions',
  'tabMenu.close': 'Close',
  'tabMenu.closeOthers': 'Close others',
  'tabMenu.closeAll': 'Close all',

  // ---------- Encoding menu ----------
  'encoding.switch': 'Switch encoding',
  'encoding.auto': 'Auto detect',
};
