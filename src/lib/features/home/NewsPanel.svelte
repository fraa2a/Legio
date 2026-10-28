<script lang="ts">
  import { onMount } from "svelte";
  import { startVisibleRefresh } from "../../stores/visible-refresh";
  import { t, language } from "../../i18n";
  import { formatDate } from "../../utils/format";
  import { loadNews, news } from "../../stores/news";
  import type { NewsArticle } from "../../services/news";
  import Dialog from "../../components/ui/Dialog.svelte";
  import Button from "../../components/ui/Button.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import IconButton from "../../components/ui/IconButton.svelte";
  import Panel from "../../components/ui/Panel.svelte";
  import Callout from "../../components/ui/Callout.svelte";

  let selected = $state<NewsArticle | null>(null);
  const articles = $derived([...($news.data.feed?.items ?? [])].filter(article => Date.parse(article.publishedAt) <= Date.now()).sort((a,b) => Date.parse(b.publishedAt) - Date.parse(a.publishedAt)));
  const date = (value: string) => formatDate(value, $language, { dateStyle: "medium" });
  onMount(() => startVisibleRefresh(() => void loadNews(), 15 * 60 * 1000));
</script>

<Panel title={t("Novità", $language)} class="min-h-0">
  {#snippet actions()}
    <IconButton label={t("Aggiorna novità", $language)} title={t("Aggiorna", $language)} onClick={() => void loadNews(true)}><Icon name="reload" size="size-4" /></IconButton>
  {/snippet}
  <div class="min-h-0 overflow-auto [mask-image:linear-gradient(to_bottom,black_calc(100%-1.25rem),transparent)]">
    {#if articles.length > 0}
      <ul class="flex flex-col divide-y divide-white/10 light:divide-zinc-900/10">
        {#each articles.slice(0, 20) as article (article.id)}
          <li><button type="button" class="flex w-full flex-col gap-1 py-3 text-left" onclick={() => selected = article}>
            <time class="text-xs text-zinc-400 light:text-zinc-600" datetime={article.publishedAt}>{date(article.publishedAt)}</time>
            <span class="text-sm font-semibold text-zinc-100 light:text-zinc-900">{article.title[$language]}</span>
            <span class="line-clamp-2 text-xs text-zinc-400 light:text-zinc-600">{article.summary[$language]}</span>
          </button></li>
        {/each}
      </ul>
    {:else}
      <p class="text-sm text-zinc-400 light:text-zinc-600" role="status">{$news.status === "idle" || $news.status === "loading" ? t("Caricamento novità...", $language) : t("Nessuna novità disponibile.", $language)}</p>
    {/if}
    {#if $news.data.warning || $news.error}<Callout tone="warning" variant="inline" class="mt-3" message={t("Non è stato possibile aggiornare le novità. Le notizie salvate restano disponibili.", $language)} />{/if}
  </div>
</Panel>
{#if selected}
  <Dialog open title={selected.title[$language]} onClose={() => selected = null}>
  {#snippet children(dismiss)}
    {#if selected}
    <time class="text-xs text-zinc-400 light:text-zinc-600" datetime={selected.publishedAt}>{date(selected.publishedAt)}</time>
    <p class="whitespace-pre-wrap break-words text-sm leading-relaxed text-zinc-200 light:text-zinc-800">{selected.body[$language]}</p>
    <div class="flex justify-end"><Button label={t("Chiudi", $language)} variant="secondary" onClick={dismiss} /></div>
    {/if}
  {/snippet}
</Dialog>
{/if}
