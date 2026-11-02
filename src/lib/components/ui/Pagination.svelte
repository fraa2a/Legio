<script lang="ts">
  import { t, language } from "../../i18n";
  import { pageNumbers } from "../../utils/pagination";
  import Button from "./Button.svelte";
  import Icon from "./Icon.svelte";

  let { page, pageCount, hasMore = false, disabled = false, onChange }: {
    page: number;
    pageCount: number;
    // More pages exist beyond the known ones and are fetched when the next page is requested.
    hasMore?: boolean;
    disabled?: boolean;
    onChange: (page: number) => void;
  } = $props();

  let root: HTMLElement | undefined = $state();
  const numbers = $derived(pageNumbers(page, pageCount));

  function select(next: number): void {
    onChange(next);
    // The list is the element right above this control.
    root?.previousElementSibling?.scrollIntoView({ block: "start" });
  }
</script>

<nav bind:this={root} aria-label={t("Pagine", $language)} class="flex flex-wrap items-center justify-center gap-1 pt-2">
  <Button label={t("Pagina precedente", $language)} variant="secondary" size="sm" square disabled={disabled || page <= 1} onClick={() => select(page - 1)}>
    <Icon name="previous" size="size-4" />
  </Button>
  {#each numbers as item, index (item === "gap" ? `gap${index}` : item)}
    {#if item === "gap"}
      <span aria-hidden="true" class="px-1 text-xs text-zinc-500">...</span>
    {:else}
      <Button label={t("Pagina {0}", $language, [item])} variant={item === page ? "primary" : "secondary"} size="sm" square pressed={item === page} {disabled} onClick={() => select(item)}>
        {item}
      </Button>
    {/if}
  {/each}
  {#if hasMore}<span aria-hidden="true" class="px-1 text-xs text-zinc-500">...</span>{/if}
  <Button label={t("Pagina successiva", $language)} variant="secondary" size="sm" square disabled={disabled || (page >= pageCount && !hasMore)} onClick={() => select(page + 1)}>
    <Icon name="next" size="size-4" />
  </Button>
</nav>
