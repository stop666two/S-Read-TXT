// 编辑器焦点归还：弹窗/工具栏操作结束后把键盘焦点交还隐藏输入代理，
// 否则在弹窗关闭（焦点落到 body）后，Ctrl+A/退格/输入等会静默失效。
// 编辑层自身（EditLayer）另有 focusin 守卫覆盖「点击工具栏/菜单/状态栏」的场景。

/** 聚焦编辑器输入代理（不存在时静默：非编辑态属于正常情况）。 */
export function focusEditorProxy(): void {
  const proxy = document.querySelector<HTMLTextAreaElement>('textarea.input-proxy');
  proxy?.focus();
}
