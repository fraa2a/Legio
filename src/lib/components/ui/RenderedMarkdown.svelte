<script lang="ts">
  import { renderMarkdown } from "../../services/markdown";
  import { showToast } from "../../stores/toast";
  import Markdown from "./Markdown.svelte";

  let { markdown, class: className = "" }: { markdown: string; class?: string } = $props();

  let html: string | null = $state(null);

  // Remote notes are rendered and sanitized by the backend; plain text stays as the fallback until then or if rendering fails.
  $effect(() => {
    const source = markdown;
    let stale = false;
    html = null;
    renderMarkdown(source).then((result) => { if (!stale) html = result; }, () => {});
    return () => { stale = true; };
  });
</script>

{#if html !== null}
  <Markdown html={html} class={className} onError={(message) => showToast(message, "error")} />
{:else}
  <p class="whitespace-pre-line wrap-anywhere text-sm {className}">{markdown}</p>
{/if}
