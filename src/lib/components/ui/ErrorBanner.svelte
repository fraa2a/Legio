<script lang="ts">
  import { t, language } from "../../i18n";
  import Callout from "./Callout.svelte";
  let {
    message,
    onRetry,
    retryLabel = t("Riprova", $language),
    tone = "error",
  }: {
    message: string;
    onRetry?: () => void;
    retryLabel?: string;
    tone?: "error" | "warning";
  } = $props();
</script>

<Callout tone={tone === "error" ? "danger" : "warning"} {message} {onRetry} {retryLabel} />
