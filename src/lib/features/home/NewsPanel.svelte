<script lang="ts">
  import { onMount } from "svelte";
  import { startVisibleRefresh } from "../../stores/visible-refresh";
  import { t, language } from "../../i18n";
  import { loadNews, news } from "../../stores/news";
  import type { NewsArticle } from "../../services/news";
  import Dialog from "../../components/ui/Dialog.svelte";
  import Button from "../../components/ui/Button.svelte";

  let selected = $state<NewsArticle | null>(null);
  const articles = $derived([...($news.data.feed?.items ?? [])].filter(article => Date.parse(article.publishedAt) <= Date.now()).sort((a,b) => Date.parse(b.publishedAt) - Date.parse(a.publishedAt)));
  const date = (value: string) => new Intl.DateTimeFormat(undefined, { dateStyle: "medium" }).format(new Date(value));
  onMount(() => startVisibleRefresh(() => void loadNews(), 15 * 60 * 1000));
</script>

<section class="legio-glass flex min-h-0 flex-col gap-3 rounded-2xl bg-zinc-900 p-4 light:bg-zinc-100" aria-labelledby="news-title">
  <div class="flex items-center justify-between gap-2">
    <h2 id="news-title" class="text-lg font-medium text-zinc-50 light:text-zinc-900">{t("Novità", $language)}</h2>
    <button type="button" class="text-xs text-zinc-400 hover:text-zinc-100 light:hover:text-zinc-900" onclick={() => void loadNews(true)}>{t("Aggiorna", $language)}</button>
  </div>
  <div class="min-h-0 overflow-auto">
    {#if articles.length > 0}
      <ul class="flex flex-col divide-y divide-zinc-700">
        {#each articles.slice(0, 20) as article (article.id)}
          <li><button type="button" class="flex w-full flex-col gap-1 py-3 text-left" onclick={() => selected = article}>
            <time class="text-xs text-zinc-400" datetime={article.publishedAt}>{date(article.publishedAt)}</time>
            <span class="text-sm font-semibold text-zinc-100 light:text-zinc-900">{article.title[$language]}</span>
            <span class="line-clamp-2 text-xs text-zinc-400">{article.summary[$language]}</span>
          </button></li>
        {/each}
      </ul>
    {:else}
      <p class="text-sm text-zinc-400" role="status">{$news.status === "idle" || $news.status === "loading" ? t("Caricamento novità...", $language) : t("Nessuna novità disponibile.", $language)}</p>
    {/if}
    {#if $news.data.warning || $news.error}<p role="status" class="mt-3 text-xs text-amber-300 light:text-amber-800">{t("Non è stato possibile aggiornare le novità. Le notizie salvate restano disponibili.", $language)}</p>{/if}
  </div>
</section>
{#if selected}
  <Dialog open title={selected.title[$language]} onClose={() => selected = null}>
  {#snippet children(dismiss)}
    {#if selected}
    <time class="text-xs text-zinc-400" datetime={selected.publishedAt}>{date(selected.publishedAt)}</time>
    <p class="mt-4 whitespace-pre-wrap break-words text-sm leading-relaxed text-zinc-200 light:text-zinc-800">{selected.body[$language]}</p>
    <div class="mt-6 flex justify-end"><Button label={t("Chiudi", $language)} variant="secondary" onClick={dismiss} /></div>
    {/if}
  {/snippet}
</Dialog>
{/if}
