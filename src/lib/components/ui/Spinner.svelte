<script lang="ts">
  let { size = "md" }: { size?: "sm" | "md" } = $props();
</script>

<span
  class="{size === 'sm' ? 'size-3.5' : 'size-4'} shrink-0 animate-spin rounded-full border-2 border-zinc-400 border-t-transparent light:border-zinc-500 light:border-t-transparent"
  aria-hidden="true"
></span>
