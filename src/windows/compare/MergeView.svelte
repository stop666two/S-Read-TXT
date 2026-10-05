<script lang="ts">
  // 三方合并视图（占位实现：完整交互在后续任务落地）。
  import type { MergeDocs } from '../../lib/ipc';

  let { merge }: { merge: MergeDocs } = $props();
</script>

<div class="merge" data-merge-view>
  <div class="summary">
    {merge.baseName} · {merge.oursName} · {merge.theirsName}
  </div>
</div>

<style>
  .merge {
    padding: 12px;
  }
  .summary {
    font-size: 12.5px;
    opacity: 0.85;
  }
</style>
