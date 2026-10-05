// IPC 封装：类型化命令包装 + 错误归一（Tauri 拒绝值即后端 IpcError 载荷）。
// 约定：组件与 store 一律经本模块访问后端，不直接 import invoke，便于类型与错误策略统一。

import { invoke } from '@tauri-apps/api/core';

import { t } from './i18n/runtime';

/** 应用信息（与 Rust commands.rs 的 AppInfo 对齐）。 */
export interface AppInfo {
  version: string;
  dataDir: string;
  dataDirOrigin: 'portable' | 'envOverride' | 'runtimeOverride' | 'persisted';
}

/** 数据目录状态（data_dir_status / set_data_dir 返回体；与 Rust DataDirStatus 对齐）。 */
export interface DataDirStatus {
  /** 数据目录绝对路径 */
  dir: string;
  /** 是否可写（false 时 message 给出原因） */
  writable: boolean;
  /** 不可写原因（可写时为 null/缺省） */
  message?: string | null;
  /** 目录来源：便携 / 环境变量覆盖 / 会话级运行时覆盖 / 持久化指针 */
  origin: 'portable' | 'envOverride' | 'runtimeOverride' | 'persisted';
  /** 持久化指针目标（程序目录 config.json；未设置时为 null/缺省） */
  persisted?: string | null;
}

/** 单项文本行（与 Rust textfile::window::RowText 对齐）。 */
export interface RowText {
  row: number;
  text: string;
  /** 逻辑行号（编辑态超长行分段时存在；普通行与 row 相同） */
  logicalRow?: number;
  /** 段首逻辑行内 UTF-16 偏移（编辑态超长行分段时存在；普通行为 0 可省略） */
  baseUtf16?: number;
}

/** 标签信息（与 Rust app_state::TabInfo 对齐）。 */
export interface TabInfo {
  tabId: number;
  /** 所属窗口 label（多窗口：`main` / `main-2`…） */
  owner: string;
  path: string;
  name: string;
  /** 未命名标签序号（新建文件专用） */
  untitled?: number;
  encoding: string;
  encodingOverride: string | null;
  /** 是否处于编辑模式（编辑文档已创建且模式开关为开） */
  editing: boolean;
  /** 是否有未保存修改 */
  dirty: boolean;
  /** 是否只读（文件超过只读阈值：可浏览、不可进入编辑） */
  readOnly: boolean;
  rowsTotal: number;
  byteLen: number;
  /** 换行符风格（lf/crlf/cr/mixed/unknown） */
  eol: string;
}

/** 取行载荷（与 Rust app_state::RowsPayload 对齐）。 */
export interface RowsPayload {
  tabId: number;
  startRow: number;
  rowsTotal: number;
  startPercent: number;
  rows: RowText[];
}

/** 标签视图（与 Rust commands::TabsView 对齐）。 */
export interface TabsView {
  tabs: TabInfo[];
  activeTabId: number | null;
}

/** 编辑操作（与 Rust textfile::editing::edit_doc::EditOp 对齐；kind 为外部标签）。 */
export type EditOp =
  | { kind: 'insert'; row: number; utf16: number; text: string }
  | { kind: 'delete'; startRow: number; startUtf16: number; endRow: number; endUtf16: number }
  | {
      kind: 'replace';
      startRow: number;
      startUtf16: number;
      endRow: number;
      endUtf16: number;
      text: string;
    };

/** 一次编辑应用结果（与 Rust EditApplied 对齐）。 */
export interface EditApplied {
  stateId: number;
  dirty: boolean;
  touchedRow: number;
  rowsTotal: number;
  byteLen: number;
  /** 应用后的光标显示行（后端换算，长行分段场景权威落点） */
  caretRow: number;
  /** 应用后的光标段内 UTF-16 偏移 */
  caretUtf16: number;
}

/** 查找命中（显示行坐标；超长行分段对前端透明）。 */
export interface FindHit {
  startRow: number;
  startUtf16: number;
  endRow: number;
  endUtf16: number;
}

/** 替换一次的结果（next = 后续命中，无则 null）。 */
export interface ReplaceNextOutcome {
  applied: EditApplied;
  next: FindHit | null;
}

/** 全部替换的结果（无命中时 applied 为 null）。 */
export interface ReplaceAllOutcome {
  replaced: number;
  applied: EditApplied | null;
}

/** 查找模式：标准（字面）或正则（用户自写 Rust regex 语法）。 */
export type SearchMode = 'literal' | 'regex';

/** 「全部替换」预览中的单条命中（确认弹窗展示与逐条剔除用）。 */
export interface ReplacePreviewItem {
  index: number;
  startRow: number;
  startUtf16: number;
  endRow: number;
  endUtf16: number;
  lineText: string;
  matchedText: string;
  replacementText: string;
}

/** 「全部替换」预览结果（stateId 用于执行时校验文档未变化）。 */
export interface ReplacePreview {
  stateId: number;
  total: number;
  truncated: boolean;
  items: ReplacePreviewItem[];
}

/** 批量插入/序号：编号格式（10 种；与 Rust `NumberFormat` 对齐）。 */
export type BatchNumberFormat =
  | 'arabic'
  | 'zeroPad'
  | 'chineseLower'
  | 'chineseUpper'
  | 'parenthesized'
  | 'bracketed'
  | 'circled'
  | 'circledFilled'
  | 'romanUpper'
  | 'romanLower';

/** 序号插入位置：行首 / 行尾（行尾 = 原文 + 空格 + 序号串）。 */
export type BatchInsertPosition = 'lineStart' | 'lineEnd';

/** 作用范围（tag=kind；行号均为显示行序号）。 */
export type BatchScope =
  | { kind: 'all' }
  | { kind: 'currentLine'; row: number }
  | { kind: 'rowRange'; from: number; to: number }
  | { kind: 'nonEmpty' }
  | { kind: 'selection'; from: number; to: number };

/** 批量序号配置（与 Rust `BatchNumberingConfig` 对齐）。 */
export interface BatchNumberingConfig {
  format: BatchNumberFormat;
  start: number;
  step: number;
  zeroPadWidth: number;
  separator: string;
  suffix: string;
  position: BatchInsertPosition;
  scope: BatchScope;
  skipEmpty: boolean;
  template: string | null;
  previewLines: number;
}

/** 预览单条（row=显示行序号；after=插入后的该行文本）。 */
export interface BatchPreviewItem {
  row: number;
  after: string;
  insertText: string;
}

/** 批量序号预览（truncated=仅渲染前 previewLines 条）。 */
export interface BatchPreview {
  items: BatchPreviewItem[];
  totalRows: number;
  truncated: boolean;
}

/** 批量序号执行结果（applied 用于刷新编辑视图）。 */
export interface BatchNumberingOutcome {
  applied: EditApplied;
  affected: number;
}

/** 行操作种类（与 Rust `LineOp` 对齐；26 种）。 */
export type LineOp =
  | 'moveUp'
  | 'moveDown'
  | 'duplicate'
  | 'delete'
  | 'merge'
  | 'split'
  | 'sort'
  | 'reverse'
  | 'dedupe'
  | 'removeEmptyLines'
  | 'trimLines'
  | 'trimTrailingWhitespace'
  | 'indent'
  | 'outdent'
  | 'tabsToSpaces'
  | 'spacesToTabs'
  | 'case'
  | 'widthConvert'
  | 'prependText'
  | 'appendText'
  | 'deleteHead'
  | 'deleteTail'
  | 'extractColumn'
  | 'delimiterConvert'
  | 'collapseEmptyLines'
  | 'ensureTrailingNewline';

/** 排序模式（行操作引擎与编辑器设置共用取值）。 */
export type LineSortOrder = 'lex' | 'natural' | 'length' | 'random';
/** 去重规则。 */
export type LineDedupeMode = 'keepFirst' | 'keepLast';
/** 缩进字符。 */
export type LineIndentStyle = 'spaces' | 'tab';
/** 大小写模式。 */
export type LineCaseMode = 'upper' | 'lower' | 'title';
/** 全角/半角方向。 */
export type LineWidthDirection = 'toHalf' | 'toFull';
/** 行操作默认范围（编辑器设置）。 */
export type LineScopeKind = 'all' | 'currentLine' | 'rowRange' | 'nonEmpty' | 'selection';

/** 行操作配置（与 Rust `LineOpConfig` 对齐；各操作只读取自己相关的字段）。 */
export interface LineOpConfig {
  op: LineOp;
  scope: BatchScope;
  sortOrder: LineSortOrder;
  sortSeed: number;
  dedupeMode: LineDedupeMode;
  dedupeIgnoreCase: boolean;
  dedupeFuzzy: boolean;
  indentWidth: number;
  indentStyle: LineIndentStyle;
  caseMode: LineCaseMode;
  widthDirection: LineWidthDirection;
  text: string;
  count: number;
  delimiter: string;
  delimiterTo: string;
  skipEmpty: boolean;
  previewLines: number;
}

/** 行操作预览行（`row` 为显示行序号）。 */
export interface LineOpPreviewItem {
  row: number;
  text: string;
}

/** 行操作预览（`warning` 为非致命提示，如移动已到边界）。 */
export interface LineOpPreview {
  affectedRows: number;
  truncated: boolean;
  items: LineOpPreviewItem[];
  warning: string | null;
}

/** 行操作执行结果（`applied` 为空表示无变化）。 */
export interface LineOpOutcome {
  affected: number;
  applied: EditApplied | null;
  warning: string | null;
}

/** 行操作默认值（编辑器设置「app.editor.lines」；与 Rust `LineOpsSettings` 对应）。 */
export interface EditorLinesSettings {
  defaultScope: LineScopeKind;
  sortMode: LineSortOrder;
  dedupeMode: LineDedupeMode;
  dedupeIgnoreCase: boolean;
  dedupeFuzzy: boolean;
  indentWidth: number;
  indentStyle: LineIndentStyle;
  caseDefault: LineCaseMode;
  columnDelimiter: string;
  preview: boolean;
  skipEmptyLines: boolean;
}

/** 编辑器多光标设置（与 Rust `MultiCursorSettings` 对应）。 */
export interface MultiCursorSettings {
  enabled: boolean;
  rectModifier: 'alt' | 'ctrlAlt';
  maxCount: number;
}

/** 剪贴板历史设置（与 Rust `ClipboardSettings` 对应）。 */
export interface ClipboardSettings {
  historyLimit: number;
  persist: boolean;
}

/** 计数模式（状态栏字数统计口径；与 Rust `CountMode` 对应）。 */
export type CountMode = 'grapheme' | 'codepoint' | 'byte';

/** 状态栏显示设置（与 Rust `StatusSettings` 对应）。 */
export interface StatusSettings {
  /** 显示项顺序（候选 id：lineCol/counts/words/progress/size/encoding/eol/modified） */
  items: string[];
  countMode: CountMode;
  tabWidth: number;
  clickableGoto: boolean;
  clickableEncoding: boolean;
  clickableEol: boolean;
  emptySelectionText: string;
}

/** 文本统计（与 Rust `TextStats` 对应；capped = 已达上限未全量统计）。 */
export interface TextStats {
  graphemes: number;
  codepoints: number;
  bytes: number;
  words: number;
  capped: boolean;
}

/** 换行符转换结果（与 Rust `EolConvertOutcome` 对应）。 */
export interface EolConvertOutcome {
  replacements: number;
  applied: EditApplied | null;
}

/** 时间戳插入格式（与 Rust `TimestampFormat` 对应）。 */
export type TimestampFormat = 'localDateTime' | 'dateOnly' | 'timeOnly' | 'iso8601' | 'rfc3339Utc';

/** 时间戳插入设置（与 Rust `InsertSettings` 对应）。 */
export interface InsertSettings {
  timestampFormat: TimestampFormat;
}

/** 括号匹配/自动缩进设置（与 Rust `AutoPairsSettings` 对应）。 */
export interface AutoPairsSettings {
  enabled: boolean;
  autoClose: boolean;
  autoIndent: boolean;
  highlightMatch: boolean;
}

/** 清理类操作设置（与 Rust `CleanupSettings` 对应）。 */
export interface CleanupSettings {
  trailingWhitespace: boolean;
  collapseBlankLines: boolean;
  trailingNewline: boolean;
}

export interface EditorSettings {
  lines: EditorLinesSettings;
  multiCursor: MultiCursorSettings;
  clipboard: ClipboardSettings;
  insert: InsertSettings;
  autoPairs: AutoPairsSettings;
  cleanup: CleanupSettings;
}

/** 保存结果（与 Rust commands::SaveTabResult 对齐）。 */
export interface SaveTabResult {
  bytesWritten: number;
  backupPath: string | null;
  encoding: string;
  tab: TabInfo;
}

/** 后端统一错误载荷（与 Rust ipc_error::IpcError 对齐）。 */
export interface IpcErrorPayload {
  code: string;
  message: string;
}

/**
 * 把任意异常归一为 IpcError 载荷。
 * Tauri 命令拒绝值即后端序列化的 IpcError；其余异常（网络/JS 错误）兜底为 UNKNOWN。
 */
export function toIpcError(error: unknown): IpcErrorPayload {
  if (typeof error === 'object' && error !== null) {
    const candidate = error as Partial<IpcErrorPayload>;
    if (typeof candidate.code === 'string' && typeof candidate.message === 'string') {
      return { code: candidate.code, message: candidate.message };
    }
  }
  return {
    code: 'UNKNOWN',
    message: error instanceof Error ? error.message : String(error),
  };
}

/**
 * 错误载荷 → 用户文案。
 * 固定展示文案优先（如文件过大使用需求给定的逐字提示），其余透出后端消息。
 */
export function describeIpcError(error: IpcErrorPayload): string {
  switch (error.code) {
    case 'FILE_TOO_LARGE':
      return t('error.fileTooLarge');
    case 'MAX_TABS':
      return t('error.maxTabs');
    default:
      return error.message;
  }
}

/** 历史保留策略（与 Rust `HistorySettings` 对应）。 */
export interface HistorySettings {
  maxEntries: number;
  retentionDays: number;
}

/** 启动行为（与 Rust `StartupSettings` 对应）。 */
export interface StartupSettings {
  restoreSession: boolean;
  restoreWindow: boolean;
}

/** 主配置（与 Rust `AppSettings` 对应；字段名以 Rust 序列化为准）。 */
/** 查找范围（与 Rust `FindScope` 对应）。 */
export type FindScope = 'document' | 'selection' | 'rowRange';

/** 查找设置（与 Rust `FindSettings` 对应）。 */
export interface FindSettings {
  /** 默认区分大小写 */
  caseSensitive: boolean;
  /** 默认全词匹配（仅字面模式生效） */
  wholeWord: boolean;
  /** 循环查找：到达文末后从文首继续 */
  wrapAround: boolean;
  /** 高亮全部匹配 */
  highlightAll: boolean;
  /** 显示匹配计数 */
  matchCount: boolean;
  /** 替换前预览确认 */
  replacePreview: boolean;
  /** 查找范围默认值 */
  defaultScope: FindScope;
  /** 查找历史条数上限（0 = 不留历史） */
  historyLimit: number;
  /** 多文件（工作区）搜索开关 */
  multifileEnabled: boolean;
  /** 多文件扫描并发数（1–16） */
  multifileConcurrency: number;
  /** 匹配高亮颜色（空串 = 跟随主题内置色） */
  highlightColor: string;
}

/** 正则设置（与 Rust `RegexSettings` 对应）。 */
export interface RegexSettings {
  /** 单次扫描超时（毫秒；超时中断并提示，不修改内容） */
  timeoutMs: number;
  /** 常用正则库（逐项编译校验） */
  library: string[];
}

/** 匹配计数（与 Rust `MatchCount` 对应）。 */
export interface MatchCount {
  /** 匹配总数（上限 20 万） */
  total: number;
  /** 是否已达上限（total 为下限） */
  truncated: boolean;
}

export interface FileSettings {
  newEncoding: string;
  newEol: NewEol;
  autosaveIntervalSec: number;
  autosaveWriteBack: boolean;
  snapshotKeep: number;
  snapshotMaxMB: number;
  versionHistory: boolean;
  associations: string[];
  recentLimit: number;
}

/** 新建文件换行风格（与 Rust `NewEol` 对应）。 */
export type NewEol = 'lf' | 'crlf' | 'cr';

/** 快照条目（版本历史）。 */
export interface SnapshotInfo {
  name: string;
  createdMillis: number;
  bytes: number;
}

export interface AppSettings {
  schemaVersion: number;
  logLevel: string;
  maxFileSizeMB: number;
  hardLimitMB: number;
  maxTabs: number;
  history: HistorySettings;
  saveBackupEnabled: boolean;
  showOnboarding: boolean;
  locale: 'zh-CN' | 'en';
  /** 编辑器设置 */
  editor: EditorSettings;
  file: FileSettings;
  /** 查找设置 */
  find: FindSettings;
  /** 状态栏显示设置 */
  status: StatusSettings;
  /** 显示选项 */
  display: DisplaySettings;
  /** 正则设置 */
  regex: RegexSettings;
  startup: StartupSettings;
}

/** 折叠方式。 */
export type FoldingMode = 'off' | 'indent' | 'heading' | 'regex';

/** 大纲条目（显示行坐标与渲染/跳转一致）。 */
export interface OutlineItem {
  row: number;
  title: string;
  level: number;
}

/** 折叠区间（闭区间，startRow 为可点击的折叠标记行）。 */
export interface FoldRegion {
  startRow: number;
  endRow: number;
}

/** 显示选项（与 Rust `DisplaySettings` 对应）。 */
export interface DisplaySettings {
  lineNumbers: boolean;
  relativeLineNumbers: boolean;
  highlightCurrentLine: boolean;
  wordWrap: boolean;
  ruler: boolean;
  rulerPosition: number;
  indentGuides: boolean;
  invisible: string[];
  scrollbarMarkers: boolean;
  /** 折叠方式（关闭/按缩进/按标题/按正则） */
  folding: FoldingMode;
  /** 大纲面板 */
  outline: boolean;
  /** 面包屑 */
  breadcrumb: boolean;
  /** 大纲正则（空列表后端回退内置默认） */
  outlinePatterns: string[];
}

/** 四向页边距（px；与 Rust `Margin4` 对应）。 */
export interface Margin4 {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

/** 边距设置（阅读 / 编辑两套独立；与 Rust `MarginSettings` 对应）。 */
export interface MarginSettings {
  reading: Margin4;
  editing: Margin4;
}

/** 阅读模式设置（与 Rust `ReadingSettings` 对应；P2-4）。 */
export interface ReadingSettings {
  columns: number;
  autoScrollSpeed: number;
  focusMode: boolean;
  typewriter: boolean;
  eyeCareIntervalMin: number;
  pomodoroMin: number;
  readingStats: boolean;
  progressMemory: boolean;
  pageMode: 'scroll' | 'paged' | 'double';
  pageAnimMs: number;
}

/** 排版配置（与 Rust `TypographySettings` 对应）。 */
export interface TypographySettings {
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  contentWidth: number;
  paragraphSpacing: number;
  firstLineIndent: number;
  textAlign: 'left' | 'justify';
  smoothScroll: boolean;
}

/** 过滤视图查询（与 Rust `FilterQuery` 对应）。 */
export interface FilterQuery {
  /** 匹配文本（空 = 只按 hideEmpty 过滤） */
  text: string;
  regex: boolean;
  caseSensitive: boolean;
  hideEmpty: boolean;
}

/** 过滤扫描结果（与 Rust `FilterResult` 对应；rows 为命中显示行号）。 */
export interface FilterResult {
  rows: number[];
  scanned: number;
  truncated: boolean;
}

/** 背景图填充模式（与 Rust `BackgroundFill` 对应）。 */
export type BackgroundFill = 'cover' | 'contain' | 'stretch' | 'tile';

/** 背景图设置（与 Rust `BackgroundSettings` 对应；`file` 为存储文件名）。 */
export interface BackgroundSettings {
  enabled: boolean;
  /** 存储文件名（`data/backgrounds/` 内；未选择时字段缺省） */
  file?: string | null;
  opacity: number;
  fill: BackgroundFill;
  blur: number;
  dim: number;
}

/** 背景图导入结果（与 Rust `BackgroundEntry` 对应）。 */
export interface BackgroundEntry {
  fileName: string;
  label: string;
  sizeBytes: number;
}

/** 背景图读取结果（base64 + MIME；前端拼 data URL 渲染）。 */
export interface BackgroundImageData {
  dataBase64: string;
  mime: string;
}

/** 阅读配置（与 Rust `ReaderSettings` 对应）。 */
export interface ReaderSettings {
  schemaVersion: number;
  /** 主题 id：`system` / 内置（light、dark、eye-green、paper-cream、high-contrast、minimal-gray）/ 用户主题 */
  theme: string;
  /** 主题切换过渡动画开关 */
  themeAnimEnabled: boolean;
  /** 主题切换过渡时长（ms；0 = 无过渡） */
  themeAnimMs: number;
  typography: TypographySettings;
  margins: MarginSettings;
  /** 阅读模式设置（专注/打字机/自动滚动/提醒等） */
  reading: ReadingSettings;
  background: BackgroundSettings;
}

/** 阅读时长统计（与 Rust `ReadingStats` 对应）。 */
export interface ReadingStats {
  day: string;
  todaySeconds: number;
  totalSeconds: number;
}

/** 主题清单摘要（与 Rust `ThemeSummary` 对应；名称按当前语言取 `name`/`nameEn`）。 */
export interface ThemeSummary {
  id: string;
  name: string;
  nameEn: string;
  base: 'light' | 'dark';
  builtin: boolean;
}

/** 书签（与 Rust `Bookmark` 对应；坐标为显示行 + 行内 UTF-16）。 */
export interface AnnotBookmark {
  id: number;
  row: number;
  utf16: number;
  label: string | null;
  createdAt: string;
  excerpt: string;
}

/** 高亮（同一行内的 UTF-16 半开区间）。 */
export interface AnnotHighlight {
  id: number;
  row: number;
  startUtf16: number;
  endUtf16: number;
  color: string | null;
  note: string | null;
  createdAt: string;
  excerpt: string;
}

/** 注释类型：普通注释 / 待办 / 行内批注。 */
export type NoteKind = 'note' | 'todo' | 'inline';

/** 注释（`endUtf16` 非空表示区间批注，否则为单点）。 */
export interface AnnotNote {
  id: number;
  row: number;
  utf16: number;
  endUtf16: number | null;
  text: string;
  done: boolean;
  kind: NoteKind;
  createdAt: string;
  excerpt: string;
}

/** 单文件的全部标注（与 Rust `FileAnnotations` 对应）。 */
export interface FileAnnotations {
  schemaVersion: number;
  path: string;
  nextId: number;
  bookmarks: AnnotBookmark[];
  highlights: AnnotHighlight[];
  notes: AnnotNote[];
}

/** 主题清单（与 Rust `ThemeManifest` 对应；主题编辑器保存时提交）。 */
export interface ThemeManifest {
  schemaVersion: number;
  id: string;
  name: string;
  nameEn: string;
  base: 'light' | 'dark';
  builtin: boolean;
  tokens: Record<string, string>;
}

/** 解析后的主题（与 Rust `ResolvedTheme` 对应；`tokens` 为颜色令牌，键为 camelCase）。 */
export interface ResolvedTheme {
  id: string;
  base: 'light' | 'dark';
  tokens: Record<string, string>;
}

/** 自定义字体条目（与 Rust `FontEntry` 对应）。 */
export interface FontEntry {
  fileName: string;
  label: string;
  sizeBytes: number;
}

/** 快捷键配置（与 Rust `ShortcutSettings` 对应；bindings 为「生效绑定」）。 */
export interface ShortcutSettings {
  schemaVersion: number;
  bindings: Record<string, string>;
}

/** 历史记录条目（与 Rust `HistoryEntry` 对应）。 */
export interface HistoryEntry {
  path: string;
  name: string;
  size: number;
  encoding: string;
  openedAt: string;
  lastRow: number;
  lastPercent: number;
}

/** 窗口状态（会话恢复用；x/y 可为空表示未定位）。 */
export interface WindowState {
  x: number | null;
  y: number | null;
  width: number;
  height: number;
  maximized: boolean;
}

/** 会话中的单个标签（恢复用；scrollRow = 顶部定位行）。 */
export interface SessionTab {
  path: string;
  encoding: string | null;
  scrollRow: number;
  editMode: boolean;
}

/** 会话状态（与 Rust `SessionState` 对应）。 */
export interface SessionState {
  schemaVersion: number;
  window: WindowState;
  activeTabIndex: number;
  tabs: SessionTab[];
}

/** 设置项类型（与 Rust `SettingKind` 对应；tag = type）。 */
export type SettingKind =
  | { type: 'number'; min: number; max: number; integer: boolean }
  | { type: 'bool' }
  | { type: 'enum'; values: string[] }
  | { type: 'text'; maxLen: number }
  | { type: 'shortcuts' }
  | { type: 'color' }
  | { type: 'stringList'; maxItems: number; maxChars: number; allowed?: string[] };

/** 设置项元数据（与 Rust `SettingSpec` 对应；标签/描述由前端按 `setting.<id>` 解析语言包）。 */
export interface SettingSpec {
  id: string;
  group: string;
  kind: SettingKind;
}

/** 重置作用域（与 Rust `ResetScope` 对应）。 */
export type ResetScope =
  | { kind: 'all' }
  | { kind: 'group'; name: string }
  | { kind: 'field'; id: string };

/** 磁盘占用分项（与 Rust `DiskUsageItem` 对应）。 */
/** 数据目录迁移结果（与 Rust MigrationReport 对齐）。 */
export interface MigrationReport {
  /** 成功复制文件数 */
  copiedFiles: number;
  /** 成功复制字节数 */
  copiedBytes: number;
  /** 被占用而跳过的文件数（不影响迁移生效） */
  skipped: number;
  /** 原目录是否已同步清理（false = 延迟到下次启动） */
  oldRemoved: boolean;
}

export interface DiskUsageItem {
  key: 'logs' | 'webview' | 'fonts' | 'backups' | 'files' | 'others';
  bytes: number;
  files: number;
}

/** 磁盘占用报告（与 Rust `DiskUsageReport` 对应）。 */
export interface DiskUsageReport {
  items: DiskUsageItem[];
  totalBytes: number;
}

/** 清理结果（与 Rust `ClearResult` 对应）。 */
export interface ClearResult {
  clearedBytes: number;
  skipped: number;
}

/** 配置聚合快照（`get_settings` 返回体）。 */
export interface SettingsSnapshot {
  app: AppSettings;
  reader: ReaderSettings;
  shortcuts: ShortcutSettings;
}

/** 配置保存请求（`save_settings` 入参；shortcuts 直接传「生效绑定」扁平表）。 */
export interface SettingsSaveRequest {
  app: AppSettings;
  reader: ReaderSettings;
  shortcuts: Record<string, string>;
}

/** 类型化 IPC 命令集合（参数名与 Tauri 的 camelCase 约定一致）。 */
/** 剪贴板历史条目。 */
export interface ClipboardEntry {
  /** 条目文本（复制内容原样） */
  text: string;
  /** 复制时间（RFC 3339，UTC） */
  at: string;
}

export interface WorkspaceHit {
  /** 显示行号 */
  row: number;
  /** 段内起始 UTF-16 偏移 */
  startUtf16: number;
  /** 段内结束 UTF-16 偏移 */
  endUtf16: number;
  /** 命中行文本摘要（≤160 字符，列表展示用） */
  preview: string;
}

/** 单文件工作区命中结果。 */
export interface WorkspaceFileResult {
  tabId: number;
  name: string;
  path: string;
  editing: boolean;
  matches: WorkspaceHit[];
  total: number;
  truncated: boolean;
  timedOut: boolean;
  error: string | null;
}

/** 工作区搜索响应。 */
export interface WorkspaceSearchResponse {
  files: WorkspaceFileResult[];
  totalMatches: number;
  truncated: boolean;
}

/** 工作区替换单文件结果。 */
export interface WorkspaceReplaceFile {
  tabId: number;
  replaced: number;
  error: string | null;
}

/** 工作区替换响应。 */
export interface WorkspaceReplaceResponse {
  files: WorkspaceReplaceFile[];
  totalReplaced: number;
  skipped: number;
}

export const ipc = {
  /** 应用信息（版本 / 数据目录）。 */
  getAppInfo: () => invoke<AppInfo>('get_app_info'),
  /** 数据目录可写性状态（启动引导的数据源）。 */
  dataDirStatus: () => invoke<DataDirStatus>('data_dir_status'),
  /** 设置会话级数据目录（不可写引导；所有后续读写改路至新目录）。 */
  setDataDir: (dir: string) => invoke<DataDirStatus>('set_data_dir', { dir }),
  /** 打开文件（重复打开由后端复用标签）。 */
  openFile: (path: string) => invoke<TabInfo>('open_file', { path }),
  /** 取文本窗口（count 上限 2048）。 */
  getRows: (tabId: number, startRow: number, count: number) =>
    invoke<RowsPayload>('get_rows', { tabId, startRow, count }),
  /** 切换编码（null = 恢复自动检测）。 */
  setEncoding: (tabId: number, encoding: string | null) =>
    invoke<TabInfo>('set_encoding', { tabId, encoding }),
  /** 支持的编码列表。 */
  listEncodings: () => invoke<string[]>('list_encodings'),
  /** 全部标签视图。 */
  listTabs: () => invoke<TabsView>('list_tabs'),
  /** 关闭标签（返回剩余视图）。 */
  closeTab: (tabId: number) => invoke<TabsView>('close_tab', { tabId }),
  /** 文档统计（全文件流式；P2-1）。 */
  documentStats: (tabId: number) => invoke<TextStats>('document_stats', { tabId }),
  /** 选区统计（编辑态；半开 UTF-16 区间；P2-1）。 */
  selectionStats: (
    tabId: number,
    fromRow: number,
    fromUtf16: number,
    toRow: number,
    toUtf16: number,
  ) => invoke<TextStats>('selection_stats', { tabId, fromRow, fromUtf16, toRow, toUtf16 }),
  /** 全文档换行符转换（编辑态；P2-1d）。 */
  convertEol: (tabId: number, target: 'lf' | 'crlf' | 'cr') =>
    invoke<EolConvertOutcome>('convert_eol', { tabId, target }),
  /** 同步活动标签到后端（点击/快捷键选择后调用）。 */
  setActiveTab: (tabId: number) => invoke<void>('set_active_tab', { tabId }),
  /** 调整标签展示顺序（拖拽排序；下标记「移除后再插入」语义）。 */
  reorderTab: (tabId: number, toIndex: number) => invoke<void>('reorder_tab', { tabId, toIndex }),
  /** 打开设置窗口（已存在则聚焦；按需创建；tab 指定初始页签）。 */
  openSettings: (tab?: string) => invoke<void>('open_settings', { tab: tab ?? null }),
  /** 取走设置窗口待打开页签（读取即清空；无待办返回 null）。 */
  takeSettingsTab: () => invoke<string | null>('take_settings_tab'),
  /** 列出已导入的自定义字体。 */
  listFonts: () => invoke<FontEntry[]>('list_fonts'),
  /** 导入字体文件（复制到数据目录 fonts/；重名自动唯一化）。 */
  importFont: (path: string) => invoke<FontEntry>('import_font', { path }),
  /** 删除已导入字体。 */
  removeFont: (fileName: string) => invoke<void>('remove_font', { fileName }),
  /** 读取字体字节（Base64；前端经 FontFace 动态注册）。 */
  readFontData: (fileName: string) => invoke<string>('read_font_data', { fileName }),
  /** 切换编辑模式（首次进入创建编辑文档）。 */
  toggleEdit: (tabId: number) => invoke<TabInfo>('toggle_edit', { tabId }),
  /** 应用编辑批次（批次 = 单个撤销步）。 */
  applyEdits: (tabId: number, ops: EditOp[]) =>
    invoke<EditApplied>('apply_edits', { tabId, ops }),
  /** 撤销一步（无可撤销内容返回 null）。 */
  undoEdit: (tabId: number) => invoke<EditApplied | null>('undo_edit', { tabId }),
  /** 重做一步（无可重做内容返回 null）。 */
  redoEdit: (tabId: number) => invoke<EditApplied | null>('redo_edit', { tabId }),
  /** 保存（targetEncoding = null 表示保持当前编码；force = 冲突时覆盖）。 */
  saveTab: (tabId: number, targetEncoding: string | null, makeBackup: boolean, force: boolean) =>
    invoke<SaveTabResult>('save_tab', { tabId, targetEncoding, makeBackup, force }),
  /** 另存为（成功后标签重定向到新路径）。 */
  saveTabAs: (tabId: number, newPath: string, targetEncoding: string | null, makeBackup: boolean) =>
    invoke<SaveTabResult>('save_tab_as', { tabId, newPath, targetEncoding, makeBackup }),
  /** 从磁盘重载（丢弃未保存修改）。 */
  reloadTab: (tabId: number) => invoke<TabInfo>('reload_tab', { tabId }),
  /** 查找下一个（from = 显示坐标；不环绕，由前端在文末后从头重试）。 */
  findInEdit: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    wholeWord: boolean,
    from: [number, number] | null,
  ) =>
    invoke<FindHit | null>('find_in_edit', { tabId, query, caseSensitive, mode, wholeWord, from }),
  /** 工作区（多文件）查找：扫描全部已打开标签（编辑态跨行语义与查找条一致）。 */
  searchWorkspace: (query: string, caseSensitive: boolean, mode: SearchMode, wholeWord: boolean) =>
    invoke<WorkspaceSearchResponse>('search_workspace', { query, caseSensitive, mode, wholeWord }),
  /** 工作区（多文件）替换：仅作用于编辑态标签（逐文件单撤销步）。 */
  replaceWorkspace: (
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    wholeWord: boolean,
    replacement: string,
  ) =>
    invoke<WorkspaceReplaceResponse>('replace_workspace', {
      query,
      caseSensitive,
      mode,
      wholeWord,
      replacement,
    }),
  /** 替换一次（从 from 起）并返回后续命中；正则替换支持 $1 捕获展开。 */
  replaceInEdit: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    wholeWord: boolean,
    from: [number, number] | null,
    replacement: string,
  ) =>
    invoke<ReplaceNextOutcome | null>('replace_in_edit', {
      tabId,
      query,
      caseSensitive,
      mode,
      wholeWord,
      from,
      replacement,
    }),
  /** 全部替换（单撤销步；正常流程请用预览 + applyReplaceAll）。 */
  replaceAllInEdit: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    wholeWord: boolean,
    replacement: string,
  ) =>
    invoke<ReplaceAllOutcome>('replace_all_in_edit', {
      tabId,
      query,
      caseSensitive,
      mode,
      wholeWord,
      replacement,
    }),
  /** 生成「全部替换」预览（二次确认弹窗数据源；命中过多时报 QUERY_TOO_BROAD）。 */
  previewReplaceAll: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    wholeWord: boolean,
    replacement: string,
  ) =>
    invoke<ReplacePreview>('preview_replace_all_in_edit', {
      tabId,
      query,
      caseSensitive,
      mode,
      wholeWord,
      replacement,
    }),
  /** 执行「全部替换」：selected=null 全部；数组 = 仅替换所列序号（预览剔除后）。 */
  applyReplaceAll: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    wholeWord: boolean,
    replacement: string,
    selected: number[] | null,
    expectStateId: number,
  ) =>
    invoke<ReplaceAllOutcome>('apply_replace_all_in_edit', {
      tabId,
      query,
      caseSensitive,
      mode,
      wholeWord,
      replacement,
      selected,
      expectStateId,
    }),
  /** 显示行窗口内的命中（文档高亮用；扫描越过窗口即停止）。 */
  matchWindow: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    wholeWord: boolean,
    startRow: number,
    count: number,
  ) =>
    invoke<FindHit[]>('match_window_in_edit', {
      tabId,
      query,
      caseSensitive,
      mode,
      wholeWord,
      startRow,
      count,
    }),
  /** 统计全文档匹配数（上限 20 万；truncated = 已达上限）。 */
  countMatchesInEdit: (
    tabId: number,
    query: string,
    caseSensitive: boolean,
    mode: SearchMode,
    wholeWord: boolean,
  ) =>
    invoke<MatchCount>('count_matches_in_edit', { tabId, query, caseSensitive, mode, wholeWord }),
  /** 查找历史（去重置顶；上限取设置）。 */
  listFindHistory: () => invoke<string[]>('list_find_history'),
  /** 记录一条查找历史（返回最新列表）。 */
  addFindHistory: (query: string) => invoke<string[]>('add_find_history', { query }),
  /** 清空查找历史（返回空列表）。 */
  clearFindHistory: () => invoke<string[]>('clear_find_history'),
  /** 预览批量序号（容量预检以 BATCH_INVALID 报错）。 */
  previewBatchNumbering: (tabId: number, config: BatchNumberingConfig) =>
    invoke<BatchPreview>('preview_batch_numbering', { tabId, config }),
  /** 执行批量序号（单撤销步）。 */
  applyBatchNumbering: (tabId: number, config: BatchNumberingConfig) =>
    invoke<BatchNumberingOutcome>('apply_batch_numbering', { tabId, config }),
  /** 预览行操作（无变化/边界等以 warning 呈现；参数非法报 LINE_OP_INVALID）。 */
  previewLineOp: (tabId: number, config: LineOpConfig) =>
    invoke<LineOpPreview>('preview_line_op', { tabId, config }),
  /** 执行行操作（单撤销步）。 */
  applyLineOp: (tabId: number, config: LineOpConfig) =>
    invoke<LineOpOutcome>('apply_line_op', { tabId, config }),
  /** 过滤扫描（只读；返回命中显示行号）。 */
  filterRows: (tabId: number, query: FilterQuery) =>
    invoke<FilterResult>('filter_rows', { tabId, query }),
  /** 稀疏按行取文本（过滤视图虚拟窗口；单次 ≤512 行）。 */
  fetchRowsAt: (tabId: number, rows: number[]) =>
    invoke<RowsPayload['rows']>('fetch_rows_at', { tabId, rows }),
  /** 大纲提取（空正则列表后端回退内置默认）。 */
  outlineItems: (tabId: number) => invoke<OutlineItem[]>('outline_items', { tabId }),
  /** 折叠区间（按当前折叠设置计算）。 */
  foldRegions: (tabId: number) => invoke<FoldRegion[]>('fold_regions', { tabId }),
  listSnapshots: (tabId: number) => invoke<SnapshotInfo[]>('list_snapshots', { tabId }),
  createSnapshot: (tabId: number) => invoke<SnapshotInfo | null>('create_snapshot', { tabId }),
  restoreSnapshot: (tabId: number, name: string) =>
    invoke<EditApplied>('restore_snapshot', { tabId, name }),
  deleteSnapshot: (tabId: number, name: string) =>
    invoke<boolean>('delete_snapshot', { tabId, name }),
  markCleanExit: () => invoke<void>('mark_clean_exit'),
  takeCrashFlag: () => invoke<boolean>('take_crash_flag'),
  /** 取走命令行/单实例待打开文件。 */
  takeCliFiles: () => invoke<string[]>('take_cli_files'),
  newFile: () => invoke<TabInfo>('new_file'),
  /** 新建主窗口；可选物理坐标与尺寸（缺省级联偏移/1100×760）；返回新窗口 label。 */
  newWindow: (options?: { x?: number; y?: number; width?: number; height?: number }) =>
    invoke<string>('new_window', {
      x: options?.x ?? null,
      y: options?.y ?? null,
      width: options?.width ?? null,
      height: options?.height ?? null,
    }),
  exportText: (tabId: number, path: string) => invoke<number>('export_text', { tabId, path }),
  printDocument: (tabId: number) => invoke<void>('print_document', { tabId }),
  /** 剪贴板历史（读取最新列表）。 */
  listClipboardHistory: () => invoke<ClipboardEntry[]>('list_clipboard_history'),
  /** 记录一次复制到历史（空文本/禁用时后端 no-op；返回最新列表）。 */
  addClipboardEntry: (text: string) => invoke<ClipboardEntry[]>('add_clipboard_entry', { text }),
  /** 删除历史单条（返回最新列表）。 */
  removeClipboardEntry: (index: number) =>
    invoke<ClipboardEntry[]>('remove_clipboard_entry', { index }),
  /** 清空剪贴板历史（返回空列表）。 */
  clearClipboardHistory: () => invoke<ClipboardEntry[]>('clear_clipboard_history'),
  /** 配置快照（快捷键等；后端为唯一真源）。 */
  getSettings: () => invoke<SettingsSnapshot>('get_settings'),
  /** 保存配置（返回保存后的快照）。 */
  saveSettings: (request: SettingsSaveRequest) =>
    invoke<SettingsSnapshot>('save_settings', { request }),
  /** 设置项注册表（设置界面动态生成 / 导入校验 / 重置作用域的唯一元数据源）。 */
  getSettingsRegistry: () => invoke<SettingSpec[]>('get_settings_registry'),
  /** 导出全部设置到指定路径（返回写入字节数）。 */
  exportSettings: (path: string) => invoke<number>('export_settings', { path }),
  /** 从导出文件导入设置（强校验；返回导入后的快照）。 */
  importSettings: (path: string) => invoke<SettingsSnapshot>('import_settings', { path }),
  exportShortcuts: (path: string) => invoke<number>('export_shortcuts', { path }),
  importShortcuts: (path: string) => invoke<SettingsSnapshot>('import_shortcuts', { path }),
  /** 重置设置（全部 / 分组 / 单项；返回重置后的快照）。 */
  resetSettings: (scope: ResetScope) => invoke<SettingsSnapshot>('reset_settings', { scope }),
  listThemes: () => invoke<ThemeSummary[]>('list_themes'),
  getTheme: (id?: string | null) => invoke<ResolvedTheme>('get_theme', { id: id ?? null }),
  importTheme: (path: string) => invoke<ThemeSummary>('import_theme', { path }),
  exportTheme: (id: string, path: string) => invoke<void>('export_theme', { id, path }),
  removeTheme: (id: string) => invoke<void>('remove_theme', { id }),
  saveTheme: (manifest: ThemeManifest) => invoke<ThemeSummary>('save_theme', { manifest }),
  listAnnotations: (tabId: number) => invoke<FileAnnotations>('list_annotations', { tabId }),
  addAnnotationBookmark: (tabId: number, row: number, utf16: number, label: string | null = null) =>
    invoke<FileAnnotations>('add_bookmark', { tabId, row, utf16, label }),
  removeAnnotationBookmark: (tabId: number, id: number) =>
    invoke<FileAnnotations>('remove_bookmark', { tabId, id }),
  addAnnotationHighlight: (
    tabId: number,
    row: number,
    startUtf16: number,
    endUtf16: number,
    color: string | null = null,
    note: string | null = null,
  ) => invoke<FileAnnotations>('add_highlight', { tabId, row, startUtf16, endUtf16, color, note }),
  removeAnnotationHighlight: (tabId: number, id: number) =>
    invoke<FileAnnotations>('remove_highlight', { tabId, id }),
  addAnnotationNote: (
    tabId: number,
    row: number,
    utf16: number,
    endUtf16: number | null,
    text: string,
    kind: NoteKind,
  ) => invoke<FileAnnotations>('add_note', { tabId, row, utf16, endUtf16, text, kind }),
  updateAnnotationNote: (tabId: number, id: number, text: string, done: boolean) =>
    invoke<FileAnnotations>('update_note', { tabId, id, text, done }),
  removeAnnotationNote: (tabId: number, id: number) =>
    invoke<FileAnnotations>('remove_note', { tabId, id }),
  clearAnnotations: (tabId: number) => invoke<FileAnnotations>('clear_annotations', { tabId }),
  editDisplayPos: (tabId: number, row: number, utf16: number) =>
    invoke<[number, number]>('edit_display_pos', { tabId, row, utf16 }),
  getReadingStats: () => invoke<ReadingStats>('get_reading_stats'),
  addReadingSeconds: (seconds: number) => invoke<ReadingStats>('add_reading_seconds', { seconds }),
  getDiskUsage: () => invoke<DiskUsageReport>('get_disk_usage'),
  clearCache: (scope: 'logs' | 'webview' | 'backups') => invoke<ClearResult>('clear_cache', { scope }),
  setBackgroundFile: (path: string) => invoke<BackgroundEntry>('set_background_file', { path }),
  clearBackgroundFile: (fileName: string) => invoke<void>('clear_background_file', { fileName }),
  readBackgroundImage: (fileName: string) =>
    invoke<BackgroundImageData>('read_background_image', { fileName }),
  /** 迁移数据目录（复制校验后写指针；需重启生效）。 */
  migrateDataDir: (target: string) => invoke<MigrationReport>('migrate_data_dir', { target }),
  /** 重启应用（迁移后立即生效；当前进程退出并由新进程接管）。 */
  restartApp: () => invoke<void>('restart_app'),
  /** 默认快捷键表（设置界面「恢复默认」用）。 */
  getDefaultShortcuts: () => invoke<Record<string, string>>('get_default_shortcuts'),
  /** 历史记录（去重剪枝后、时间倒序）。 */
  getHistory: () => invoke<HistoryEntry[]>('get_history'),
  /** 删除单条历史（以文件路径为键；返回更新后的列表）。 */
  removeHistory: (filePath: string) => invoke<HistoryEntry[]>('remove_history', { filePath }),
  /** 清空历史记录。 */
  clearHistory: () => invoke<void>('clear_history'),
  /** 更新历史条目阅读进度（关闭标签/退出前调用；尽力而为，幂等）。 */
  updateHistoryProgress: (filePath: string, lastRow: number, lastPercent: number) =>
    invoke<void>('update_history_progress', { path: filePath, lastRow, lastPercent }),
  /** 读取会话（窗口/标签/滚动；无会话返回默认值）。 */
  getSession: () => invoke<SessionState>('get_session'),
  /** 保存会话（返回归一化后的结果）。 */
  saveSession: (session: SessionState) => invoke<SessionState>('save_session', { session }),
};
