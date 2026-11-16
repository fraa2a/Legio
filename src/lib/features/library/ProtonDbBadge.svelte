<script lang="ts">
  import { t, language } from "../../i18n";
  import Badge from "../../components/ui/Badge.svelte";
  import { tierLabel, tierMeaning, type RatedTier } from "./protondb";

  let { tier, onOpen }: { tier: RatedTier; onOpen: () => void } = $props();
</script>

<button
  type="button"
  class="rounded-full"
  title={t(tierMeaning[tier], $language)}
  aria-label={t("ProtonDB: {0}. Apri la pagina del gioco", $language, [t(tierLabel[tier], $language)])}
  onclick={onOpen}
>
  <Badge tone="tier-{tier}" title="ProtonDB {t(tierLabel[tier], $language)}" />
</button>
