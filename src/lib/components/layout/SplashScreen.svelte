<script lang="ts">
  import Logo from "../ui/Logo.svelte";
</script>

<div
  class="flex h-dvh w-full items-center justify-center bg-black text-zinc-100 light:bg-zinc-200 light:text-zinc-900"
  data-tauri-drag-region
>
  <div class="flex flex-col items-center gap-8">
    <div class="flex flex-col items-center gap-4">
      <Logo size="h-20 w-20" />
      <span class="font-mono text-5xl font-bold tracking-[0.12em]">LEGIO</span>
    </div>
    <div class="flex items-center gap-3" role="status">
      <span
        class="size-5 shrink-0 animate-spin rounded-full border-2 border-white/20 border-t-white light:border-zinc-900/20 light:border-t-zinc-900"
        aria-hidden="true"
      ></span>
      <span class="text-sm text-zinc-400 light:text-zinc-600">Caricamento</span>
    </div>
  </div>
</div>
