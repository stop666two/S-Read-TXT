<script lang="ts">
  // 阅读区（阶段 2b 为静态排版示例；虚拟滚动与真实内容在阶段 2c 接线）。
  // 排版全部取自 CSS 变量（--reading-*），后续设置窗口调节即时生效。
  // 示例文本为自撰测试文案，不涉及第三方版权内容。
</script>

<div class="reader">
  <div class="page">
    <p>阅读区示例</p>
    <p>这是 S-Read-TXT 的阅读界面预览文本，用于确认排版观感：行高、限宽、字号与页边距。</p>
    <p>正文使用微软雅黑，字号 16 像素，行高 1.8 倍，阅读列宽 720 像素，页边距 48 像素。</p>
    <p>打开真实的 TXT 文件后，这里会显示文件内容；超长文件按需分块读取，不会整体载入内存。</p>
    <p>滚动到任意位置，状态栏会同步显示阅读百分比与文件编码。</p>
    <p>支持浅色、深色、护眼三种主题，也可以跟随系统外观自动切换，长时间阅读更舒适。</p>
    <p>请切换工具栏上的主题按钮查看不同配色效果，并在界面评审中提出间距、圆角与密度方面的调整意见。</p>
  </div>
</div>

<style>
  .reader {
    flex: 1;
    overflow-y: auto;
    background: var(--base);
  }

  .page {
    max-width: var(--reading-width);
    margin: 0 auto;
    padding: var(--reading-pad-y) var(--reading-pad-x);
    font-family: var(--font-reading);
    font-size: var(--reading-size);
    line-height: var(--reading-line-height);
    color: var(--ink);
  }

  p {
    margin: 0 0 0.8em;
  }
</style>
