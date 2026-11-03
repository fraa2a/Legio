<script lang="ts">
  import { openModExternalLink } from "../../services/modding";
  import { toMessage } from "../../utils/errors";

  // `html` is produced by the backend sanitizer: safe tags and web links only,
  // including remote images shown inline.
  let { html, onError }: { html: string; onError: (message: string) => void } = $props();

  // Links open in the system browser; the webview never navigates.
  function openLink(event: MouseEvent): void {
    if (!(event.target instanceof Element)) return;
    const anchor = event.target.closest("a");
    if (anchor === null) return;
    event.preventDefault();
    openModExternalLink(anchor.href).catch((error: unknown) => onError(toMessage(error)));
  }
</script>

<!-- Keyboard activation of a link also dispatches click, so one handler covers both. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="mod-markdown rounded-lg bg-white/5 p-4 text-sm light:bg-zinc-900/5" onclick={openLink} onauxclick={openLink}>
  <!-- eslint-disable-next-line svelte/no-at-html-tags -- sanitized by the backend renderer -->
  {@html html}
</div>

<style>
  .mod-markdown {
    --md-muted: color-mix(in srgb, var(--legio-text) 65%, transparent);
    --md-border: color-mix(in srgb, var(--legio-text) 18%, transparent);
    --md-code: color-mix(in srgb, var(--legio-text) 10%, transparent);
    overflow-wrap: anywhere;
    line-height: 1.6;
  }
  .mod-markdown :global(* + *) { margin-top: 0.75rem; }
  .mod-markdown :global(li + li) { margin-top: 0.25rem; }
  .mod-markdown :global(h1),
  .mod-markdown :global(h2),
  .mod-markdown :global(h3),
  .mod-markdown :global(h4),
  .mod-markdown :global(h5),
  .mod-markdown :global(h6) { font-weight: 600; line-height: 1.3; margin-top: 1.25rem; }
  .mod-markdown :global(h1) { font-size: 1.35rem; }
  .mod-markdown :global(h2) { font-size: 1.15rem; border-bottom: 1px solid var(--md-border); padding-bottom: 0.25rem; }
  .mod-markdown :global(h3) { font-size: 1.05rem; }
  .mod-markdown :global(a) { color: var(--color-legio-info); text-decoration: underline; }
  .mod-markdown :global(img) { max-width: 100%; height: auto; border-radius: 0.5rem; }
  .mod-markdown :global(ul),
  .mod-markdown :global(ol) { padding-left: 1.5rem; }
  .mod-markdown :global(ul) { list-style: disc; }
  .mod-markdown :global(ol) { list-style: decimal; }
  .mod-markdown :global(blockquote) { border-left: 3px solid var(--md-border); padding-left: 0.75rem; color: var(--md-muted); }
  .mod-markdown :global(code) { background: var(--md-code); border-radius: 0.25rem; padding: 0.1rem 0.3rem; font-size: 0.85em; }
  .mod-markdown :global(pre) { background: var(--md-code); border-radius: 0.5rem; padding: 0.75rem; overflow-x: auto; }
  .mod-markdown :global(pre code) { background: none; padding: 0; }
  .mod-markdown :global(hr) { border-color: var(--md-border); }
  .mod-markdown :global(table) { display: block; max-width: 100%; overflow-x: auto; border-collapse: collapse; }
  .mod-markdown :global(th),
  .mod-markdown :global(td) { border: 1px solid var(--md-border); padding: 0.35rem 0.6rem; text-align: left; }
  .mod-markdown :global(th) { font-weight: 600; }
</style>
