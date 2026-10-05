<script lang="ts">
  // 极简内联图标：stroke=currentColor、24 视口、不引入任何图标库。
  // 新增图标时在 Props 的联合类型与 {#if} 分支中同步登记。
  interface Props {
    /** 图标名 */
    name: 'open' | 'history' | 'palette' | 'settings' | 'close' | 'edit' | 'save' | 'plus' | 'window';
    /** 像素尺寸（默认 16） */
    size?: number;
  }
  let { name, size = 16 }: Props = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.6"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === 'open'}
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
  {:else if name === 'history'}
    <path d="M3 12a9 9 0 1 0 2.6-6.4" />
    <path d="M3 4v4h4" />
    <path d="M12 8v4l3 2" />
  {:else if name === 'palette'}
    <path
      d="M12 3a9 9 0 1 0 0 18c1.1 0 2-.9 2-2 0-.5-.2-1-.6-1.4-.4-.4-.6-.9-.6-1.4 0-1.1.9-2 2-2h1.7A3.5 3.5 0 0 0 20 10.7 9 9 0 0 0 12 3z"
    />
    <circle cx="8" cy="9" r="1" />
    <circle cx="12" cy="7" r="1" />
    <circle cx="16" cy="9" r="1" />
  {:else if name === 'settings'}
    <path d="M4 7h16" />
    <path d="M4 12h16" />
    <path d="M4 17h16" />
    <circle cx="9" cy="7" r="2" />
    <circle cx="15" cy="12" r="2" />
    <circle cx="7" cy="17" r="2" />
  {:else if name === 'close'}
    <path d="M6 6l12 12" />
    <path d="M18 6L6 18" />
  {:else if name === 'edit'}
    <path d="M12 20h9" />
    <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z" />
  {:else if name === 'save'}
    <path d="M5 3h11l3 3v15H5z" />
    <path d="M8 3v6h7V3" />
    <path d="M8 21v-6h8v6" />
  {:else if name === 'plus'}
    <path d="M12 5v14" />
    <path d="M5 12h14" />
  {:else if name === 'window'}
    <rect x="3" y="5" width="18" height="14" rx="2" />
    <path d="M3 9h18" />
  {/if}
</svg>
