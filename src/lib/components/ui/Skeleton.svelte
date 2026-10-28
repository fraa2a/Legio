<script lang="ts">
  let { class: className = "" }: { class?: string } = $props();
</script>

<span class="block animate-pulse rounded-xl bg-white/10 motion-reduce:animate-none light:bg-zinc-900/10 {className}" aria-hidden="true"></span>
