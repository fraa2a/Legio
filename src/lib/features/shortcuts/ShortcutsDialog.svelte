<script lang="ts">
  import { t, language } from "../../i18n";
  import Dialog from "../../components/ui/Dialog.svelte";
  import { sectionLabels, sections } from "../../stores/navigation";
  import { shortcutsOpen } from "../../stores/shortcuts";
  import { sectionShortcutCount } from "./shortcut-model";

  const entries: { keys: string[]; label: string }[] = $derived([
    { keys: ["Ctrl", "K"], label: t("Cerca ovunque", $language) },
    ...sections.slice(0, sectionShortcutCount).map((section, index) => ({ keys: ["Ctrl", String(index + 1)], label: t("Vai a {0}", $language, [t(sectionLabels[section], $language)]) })),
    { keys: ["Ctrl", "F"], label: t("Vai alla ricerca", $language) },
    { keys: [t("Invio", $language)], label: t("Gioca dalla pagina del gioco", $language) },
    { keys: ["?"], label: t("Mostra le scorciatoie", $language) },
  ]);
</script>

<Dialog open title={t("Scorciatoie", $language)} onClose={() => shortcutsOpen.set(false)}>
  <dl class="flex flex-col gap-2">
    {#each entries as entry (entry.label)}
      <div class="flex items-center justify-between gap-rhythm-row text-sm">
        <dt class="min-w-0 text-legio-fg">{entry.label}</dt>
        <dd class="flex shrink-0 gap-1">
          {#each entry.keys as key (key)}
            <kbd class="inline-flex h-control-sm min-w-control-sm items-center justify-center rounded-control bg-legio-fill-hover px-2 text-xs font-medium text-legio-fg">{key}</kbd>
          {/each}
        </dd>
      </div>
    {/each}
  </dl>
</Dialog>
