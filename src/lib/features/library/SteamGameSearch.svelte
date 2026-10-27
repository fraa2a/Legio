<script lang="ts">
  import { onDestroy } from "svelte";
  import { t, language } from "../../i18n";
  import { toMessage } from "../../utils/errors";
  import { cachedCatalogSearch, refreshCatalogCached, searchCatalog, type CatalogGame } from "../../services/catalog";
  import TextField from "../../components/ui/TextField.svelte";

  let { id, value = $bindable(""), disabled = false, onSelect, onQueryChange }: {
    id: string;
    value?: string;
    disabled?: boolean;
    onSelect: (game: CatalogGame) => boolean | Promise<boolean>;
    onQueryChange?: () => void;
  } = $props();

  let searchResults = $state<CatalogGame[]>([]);
  let searchStatus = $state<"idle" | "loading" | "ready" | "empty">("idle");
  let searchError = $state<string | null>(null);
  let selecting = $state(false);
  let searchRequest = 0;
  onDestroy(() => { searchRequest++; });

  async function choose(game: CatalogGame): Promise<void> {
    if (disabled || selecting) return;
    selecting = true;
    try {
      if (await onSelect(game)) value = "";
    } finally {
      selecting = false;
    }
  }

  $effect(() => {
    const query = value.trim();
    const request = ++searchRequest;
    searchResults = [];
    searchError = null;
    if (query.length < 2) {
      searchStatus = "idle";
      return;
    }
    const known = cachedCatalogSearch(query);
    if (known !== null) {
      searchResults = known.games;
      searchStatus = known.games.length > 0 ? "ready" : "empty";
      return;
    }
    searchStatus = "loading";
    const timer = setTimeout(() => {
      void (async () => {
        try {
          const cached = await searchCatalog(query);
          if (request !== searchRequest) return;
          searchResults = cached.games;
          searchStatus = cached.games.length > 0 ? "ready" : "empty";
          const fresh = await refreshCatalogCached(query);
          if (request !== searchRequest) return;
          searchResults = fresh.games;
          searchStatus = fresh.games.length > 0 ? "ready" : "empty";
        } catch (error) {
          if (request !== searchRequest) return;
          searchError = toMessage(error);
          if (searchResults.length === 0) searchStatus = "empty";
        }
      })();
    }, 300);
    return () => clearTimeout(timer);
  });
</script>

<div class="flex flex-col gap-2">
  <TextField {id} label={t("Cerca gioco su Steam", $language)} type="search" value={value} placeholder={t("Cerca per nome", $language)} disabled={disabled || selecting}
    oninput={(next) => { value = next; onQueryChange?.(); }} />
  {#if searchStatus === "loading"}
    <p class="text-xs text-zinc-400" role="status">{t("Ricerca in corso...", $language)}</p>
  {:else if searchStatus === "empty" && searchError === null}
    <p class="text-xs text-zinc-500">{t("Nessun risultato per questa ricerca.", $language)}</p>
  {/if}
  {#if searchError !== null}<p class="text-xs text-legio-warning">{t("Ricerca non disponibile: {0}", $language, [searchError])}</p>{/if}
  {#if searchResults.length > 0 && value.trim().length >= 2}
    <ul class="max-h-36 overflow-y-auto rounded-lg border border-white/10 bg-white/5 light:border-zinc-900/10 light:bg-white" aria-label={t("Risultati Steam", $language)}>
      {#each searchResults as result (result.steamAppId)}
        <li>
          <button type="button" class="flex h-12 w-full items-center px-3 text-left text-sm text-zinc-200 hover:bg-white/10 focus-visible:-outline-offset-2 light:text-zinc-800 light:hover:bg-zinc-900/5"
            disabled={disabled || selecting} onclick={() => void choose(result)}>
            <span class="truncate">{result.name}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>
