<script lang="ts">
  import { t, language } from "../../i18n";
  import Callout from "./Callout.svelte";
  let {
    message,
    detail,
    onRetry,
    retryLabel = t("Riprova", $language),
    tone = "error",
  }: {
    message: string;
    detail?: string | null;
    onRetry?: () => void;
    retryLabel?: string;
    tone?: "error" | "warning";
  } = $props();
</script>

<Callout tone={tone === "error" ? "danger" : "warning"} {message} {detail} copyable {onRetry} {retryLabel} />
