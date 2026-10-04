<script lang="ts">
  import { t } from "../../i18n";
  import { onDestroy } from "svelte";
  import { loadBackground } from "../../services/appearance";
  import AnimatedBackground from "./AnimatedBackground.svelte";
  import { toMessage } from "../../utils/errors";

  let { id, animation, animationOpacity, accent }: {
    id: string | null;
    animation: "none" | "particles" | "aurora";
    animationOpacity: number;
    accent: string;
  } = $props();
  const selectedId = $derived(id);
  let url = $state<string | null>(null);
  let error = $state<string | null>(null);
  let currentUrl: string | null = null;

  $effect(() => {
    const selected = selectedId;
    let cancelled = false;
    error = null;
    if (selected === null) {
      if (currentUrl !== null) URL.revokeObjectURL(currentUrl);
      currentUrl = null;
      url = null;
    } else {
      void loadBackground(selected).then((nextUrl) => {
        if (cancelled) {
          URL.revokeObjectURL(nextUrl);
          return;
        }
        if (currentUrl !== null) URL.revokeObjectURL(currentUrl);
        currentUrl = nextUrl;
        url = nextUrl;
      }).catch((reason) => {
        if (!cancelled) {
          error = toMessage(reason);
          if (currentUrl !== null) URL.revokeObjectURL(currentUrl);
          currentUrl = null;
          url = null;
        }
      });
    }
    return () => { cancelled = true; };
  });

  $effect(() => {
    if (url !== null) document.documentElement.style.setProperty("--legio-wallpaper-image", 'url("' + url + '")');
    else document.documentElement.style.removeProperty("--legio-wallpaper-image");
  });

  function imageFailed(): void {
    error = t("Impossibile visualizzare lo sfondo salvato.");
    if (currentUrl !== null) URL.revokeObjectURL(currentUrl);
    currentUrl = null;
    url = null;
  }

  onDestroy(() => {
    document.documentElement.style.removeProperty("--legio-wallpaper-image");
    if (currentUrl !== null) URL.revokeObjectURL(currentUrl);
  });
</script>

<div class="legio-backdrop" aria-hidden="true">
  {#if url}<img src={url} alt="" class="legio-wallpaper" onerror={imageFailed} />{/if}
  {#if animation !== "none"}
    {#key animation}<AnimatedBackground kind={animation} opacity={animationOpacity} color={accent} />{/key}
  {/if}
</div>
{#if error}
  <p class="fixed bottom-3 left-3 z-50 max-w-sm rounded-lg bg-zinc-900 p-3 text-sm text-red-400" role="alert">{error}</p>
{/if}
