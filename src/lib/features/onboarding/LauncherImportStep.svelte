<script lang="ts">
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";
  import Toggle from "../../components/ui/Toggle.svelte";
  import { isImportable, launcherImport, launcherSelection, runLauncherImport, toggleGame, toggleLauncher } from "../../stores/launcher-import";
  import type { ImportItem, Launcher } from "../../services/launcher-import";

  const names: Record<Launcher, string> = { heroic: "Heroic Games Launcher", lutris: "Lutris", itch: "itch", bottles: "Bottles" };
  const resultFor = (key: string): ImportItem | undefined => $launcherImport.results.find((item) => item.key === key);
  const resultText = (item: ImportItem): string => item.status === "imported" ? (item.warning ? `${t("Importato", $language)}. ${item.warning}` : t("Importato", $language))
    : item.status === "duplicate" ? t("Già nella libreria, saltato", $language)
    : t("Non importato: {0}", $language, [item.reason]);
</script>

<p class="text-sm text-legio-fg">{t("Legio ha trovato giochi installati con altri launcher. Scegli quali aggiungere alla libreria: i file originali non vengono modificati.", $language)}</p>
{#if $launcherImport.error}<ErrorBanner message={$launcherImport.error} />{/if}
{#each $launcherImport.groups as group (group.launcher)}
  {@const selection = launcherSelection(group, $launcherImport.selected)}
  <section class="flex flex-col gap-2" aria-label={names[group.launcher]}>
    <div class="flex items-center justify-between gap-4">
      <h2 class="text-sm font-semibold text-legio-fg">{names[group.launcher]} <span class="font-normal text-legio-fg-muted">({group.games.length})</span></h2>
      <div class="flex items-center gap-3">
        <span class="text-xs text-legio-fg-muted">{t("{0} di {1} selezionati", $language, [selection.count, selection.total])}</span>
        <Toggle label={t("Seleziona tutti", $language)} checked={selection.all} disabled={selection.total === 0 || $launcherImport.importing} onChange={() => toggleLauncher(group.launcher)} />
      </div>
    </div>
    <ul class="flex flex-col gap-2">
      {#each group.games as game (game.key)}
        {@const result = resultFor(game.key)}
        <li class="rounded-card border border-legio-line bg-legio-fill p-4">
          <Toggle label={game.title} checked={$launcherImport.selected.includes(game.key)} disabled={!isImportable(game) || $launcherImport.importing} onChange={() => toggleGame(game.key)} />
          <p class="mt-1 break-all text-xs text-legio-fg-muted">{game.installDir}</p>
          {#if game.runnerHint}<p class="text-xs text-legio-fg-muted">{t("Runner: {0}", $language, [game.runnerHint])}</p>{/if}
          {#if !isImportable(game)}<p class="text-xs text-legio-warning">{t("Non importabile: il launcher non indica l'eseguibile.", $language)}</p>{/if}
          {#if result}<p class="text-xs {result.status === 'failed' ? 'text-legio-error' : 'text-legio-fg-muted'}" role="status">{resultText(result)}</p>{/if}
        </li>
      {/each}
    </ul>
  </section>
{/each}
<div>
  <Button label={$launcherImport.importing ? t("Importazione...", $language) : t("Importa i giochi selezionati", $language)} disabled={$launcherImport.importing || $launcherImport.selected.length === 0} onClick={() => void runLauncherImport()} />
</div>
