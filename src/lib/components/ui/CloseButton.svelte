<script lang="ts">
  import Icon from "./Icon.svelte";
  let { label, onClick }: { label: string; onClick: () => void } = $props();
</script>

<button type="button" aria-label={label} onclick={onClick}
  class="rounded-lg p-1.5 text-zinc-400 transition-colors hover:bg-white/10 hover:text-zinc-100 light:text-zinc-500 light:hover:bg-zinc-900/10 light:hover:text-zinc-900">
  <Icon name="close" size="size-5" />
</button>
