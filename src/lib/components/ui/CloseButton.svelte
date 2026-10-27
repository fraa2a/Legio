<script lang="ts">
  import Icon from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";
  let { label, onClick }: { label: string; onClick: () => void } = $props();
</script>

<IconButton {label} {onClick}><Icon name="close" /></IconButton>
