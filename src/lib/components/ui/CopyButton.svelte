<script lang="ts">
  import { onDestroy } from "svelte";
  import { t, language } from "../../i18n";
  import { showToast } from "../../stores/toast";
  import Icon from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";

  const confirmationDuration = 2000;

  let {
    text,
    label = t("Copia", $language),
    size = "sm",
    class: className = "",
  }: {
    text: string;
    label?: string;
    size?: "inline-sm" | "inline-md" | "sm" | "md";
    class?: string;
  } = $props();

  let copied = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function copy(): Promise<void> {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      showToast(t("Impossibile copiare negli appunti.", $language), "error");
      return;
    }
    copied = true;
    clearTimeout(timer);
    timer = setTimeout(() => (copied = false), confirmationDuration);
  }

  onDestroy(() => clearTimeout(timer));
</script>

<IconButton {size} label={copied ? t("Copiato", $language) : label} title={copied ? undefined : label} class={className} onClick={() => void copy()}>
  <Icon name={copied ? "check" : "copy"} size="sm" />
</IconButton>
<span class="sr-only" role="status">{copied ? t("Copiato", $language) : ""}</span>
