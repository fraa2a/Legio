<script lang="ts">
  import { get } from "svelte/store";
  import { t, language } from "../../i18n";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import { activePalette } from "../../services/appearance";
  import { appearancePreview } from "../../stores/appearance";
  import { currentThemeLink, dismissThemeLink } from "../../stores/source-links";
  import { settings, updateSettings } from "../../stores/settings";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";

  let pending = $state(false);
  let error = $state<string | null>(null);

  const profile = $derived($currentThemeLink);
  // Wallpapers stay local, so the shared look keeps the current background.
  const applied = $derived(profile === null ? null : { theme: profile.theme, appearance: { ...profile.appearance, background: $settings.data.appearance.background } });
  const preset = $derived(profile === null ? null : activePalette(profile.theme, true));
  const name = $derived(profile === null || preset === null ? "" : profile.theme === "system" ? t("Sistema", $language) : t(preset.name, $language));

  $effect(() => {
    if (applied === null) return;
    const previous = get(appearancePreview);
    error = null;
    appearancePreview.set(applied);
    return () => appearancePreview.set(previous);
  });

  async function confirm(value: NonNullable<typeof applied>): Promise<void> {
    if (pending) return;
    pending = true;
    error = null;
    try {
      await updateSettings(value);
      showToast(t("Aspetto applicato.", get(language)), "success");
      dismissThemeLink();
    } catch (reason) {
      error = toMessage(reason);
    } finally {
      pending = false;
    }
  }
</script>

{#if applied !== null && preset !== null}
  <ConfirmDialog
    title={t("Applicare questo tema?", $language)}
    message={t("Un collegamento propone un aspetto per Legio. È già in anteprima: annulla per tornare al tuo.", $language)}
    confirmLabel={t("Applica tema", $language)}
    {pending}
    {error}
    onConfirm={() => void confirm(applied)}
    onClose={dismissThemeLink}
  >
    <div class="flex items-center gap-3 rounded-control bg-legio-fill p-3">
      {#if profile?.theme !== "system"}
        <span class="flex shrink-0 overflow-hidden rounded-full border border-legio-line" aria-hidden="true">
          {#each [preset.palette.background, preset.palette.surface, preset.palette.raised, preset.palette.accent] as color, index (index)}
            <span class="size-6" style:background-color={color}></span>
          {/each}
        </span>
      {/if}
      <p class="min-w-0 truncate text-sm font-medium text-legio-fg">{name}</p>
    </div>
  </ConfirmDialog>
{/if}
