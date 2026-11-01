<script lang="ts">
  import Icon from "../../components/ui/Icon.svelte";
  import { acquireModIcon } from "../../services/mod-icons";
  import { observeVisibility } from "../../utils/visibility";

  let { source, class: className = "size-12 rounded-lg" }: { source: string | null; class?: string } = $props();

  let frame: HTMLSpanElement | undefined = $state();
  let visible = $state(false);
  let url = $state<string | null>(null);

  $effect(() => observeVisibility(frame, () => (visible = true)));

  $effect(() => {
    url = null;
    if (!visible || source === null || source === "") return;
    const icon = acquireModIcon(source);
    let cancelled = false;
    url = icon.url;
    icon.ready.then((value) => { if (!cancelled) url = value; }, (error: unknown) => {
      console.warn("Could not load mod icon", error);
    });
    return () => {
      cancelled = true;
      icon.release();
    };
  });
</script>

<span bind:this={frame} class="relative flex shrink-0 items-center justify-center overflow-hidden bg-white/5 text-zinc-500 light:bg-zinc-900/5 light:text-zinc-400 {className}" aria-hidden="true">
  {#if url !== null}
    <img src={url} alt="" draggable="false" class="size-full object-cover" onerror={() => (url = null)} />
  {:else}
    <Icon name="package" size="size-1/2" />
  {/if}
</span>
