<script lang="ts">
  import Button from "../../components/ui/Button.svelte";
  import Icon from "../../components/ui/Icon.svelte";
  import ProgressBar from "../../components/ui/ProgressBar.svelte";

  let { label, value, progressLabel, onClick }: {
    label: string;
    value: number;
    progressLabel: string;
    onClick: () => void;
  } = $props();
</script>

<Button {label} variant="download" size="hero" class="relative w-60 overflow-hidden" {onClick}>
  <Icon name="download" size="size-6" />
  <ProgressBar variant="button" {value} label={progressLabel} />
</Button>
