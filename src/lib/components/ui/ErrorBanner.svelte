<script lang="ts">
  import { t, language } from "../../i18n";
  import { untrack } from "svelte";
  import { showToast } from "../../stores/toast";
  let {
    message,
    onRetry,
    retryLabel = t("Riprova", $language),
  }: {
    message: string;
    onRetry?: () => void;
    retryLabel?: string;
  } = $props();
  $effect(() => { const current = message; if (current) untrack(() => showToast(current, "error", { onRetry, retryLabel })); });
</script>
