<script lang="ts">
  import { untrack } from "svelte";
  import { cubicOut } from "svelte/easing";
  import { Tween } from "svelte/motion";
  import { pageTransitionDuration } from "../../utils/motion";

  let { amount, format }: { amount: number; format: (amount: number) => string } = $props();

  const tween = new Tween(untrack(() => amount), { duration: pageTransitionDuration, easing: cubicOut });

  $effect(() => {
    tween.target = amount;
  });
</script>

{format(tween.current)}
