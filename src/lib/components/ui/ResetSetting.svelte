<script lang="ts">
  import Icon from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";

  let { label, disabled = false, onClick }: {
    label: string;
    disabled?: boolean;
    onClick: () => void;
  } = $props();
</script>

<IconButton {label} title={label} {disabled} {onClick}>
  <span class="transition-transform group-hover:rotate-[-35deg] group-disabled:rotate-0"><Icon name="reload" /></span>
</IconButton>
