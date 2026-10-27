<script lang="ts">
  import { t, language } from "../../i18n";
  import ConfirmDialog from "../../components/ui/ConfirmDialog.svelte";
  import { accountSwitchGame, confirmAccountSwitch, dismissAccountSwitch } from "../../stores/launch";
</script>

{#if $accountSwitchGame !== null}
  <ConfirmDialog
    title={t("Cambio account Steam", $language)}
    message={t("Steam deve essere chiuso e riavviato per usare l'account salvato di {0}. Procedere?", $language, [$accountSwitchGame.name])}
    confirmLabel={t("Riavvia e avvia", $language)}
    onConfirm={() => void confirmAccountSwitch()}
    onClose={dismissAccountSwitch}
  />
{/if}
