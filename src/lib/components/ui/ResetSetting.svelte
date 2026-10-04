<script lang="ts">
  import Icon from "./Icon.svelte";

  let { label, disabled = false, onClick }: {
    label: string;
    disabled?: boolean;
    onClick: () => void;
  } = $props();
</script>

<button type="button" aria-label={label} title={label} {disabled} onclick={onClick}
    class="group grid size-8 shrink-0 place-items-center rounded-lg text-zinc-500 transition-colors hover:bg-white/10 hover:text-zinc-100 focus-visible:text-zinc-100 disabled:cursor-not-allowed disabled:opacity-30 light:text-zinc-500 light:hover:bg-zinc-900/10 light:hover:text-zinc-900">
  <span class="transition-transform duration-200 group-hover:rotate-[-35deg] group-disabled:rotate-0"><Icon name="reload" size="size-4" /></span>
</button>
