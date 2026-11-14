<script lang="ts">
  import { t, language } from "../../i18n";
  import Button from "../../components/ui/Button.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import type { GameLink } from "./game-links";
  import { openGameLink } from "./open-link";
  import ProtonDbBadge from "./ProtonDbBadge.svelte";
  import type { RatedTier } from "./protondb";

  let { links, rating = null }: { links: GameLink[]; rating?: RatedTier | null } = $props();
</script>

{#if links.length > 0}
  <nav class="flex flex-wrap items-center gap-2" aria-label={t("Collegamenti utili", $language)}>
    {#each links as link (link.kind)}
      {#if link.kind === "protondb" && rating !== null}
        <ProtonDbBadge tier={rating} onOpen={() => void openGameLink(link.url)} />
      {:else}
        <Button label={link.label} variant="secondary" size="sm" onClick={() => void openGameLink(link.url)}>
          <Icon name="external-link" size="sm" />
        </Button>
      {/if}
    {/each}
  </nav>
{/if}
