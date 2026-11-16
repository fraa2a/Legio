<script lang="ts">
  import { reducedMotion } from "../../utils/motion";
  import Icon from "./Icon.svelte";

  let { spinning }: { spinning: boolean } = $props();

  let turning = $state(false);

  $effect(() => {
    if (spinning && !reducedMotion) turning = true;
  });

  // The icon only stops on an iteration boundary, so it always completes the
  // current turn and rests at its original angle.
  function finishTurn(): void {
    if (!spinning) turning = false;
  }
</script>

<span
  class="inline-flex {turning ? 'animate-spin' : ''} {spinning && reducedMotion ? 'opacity-50' : ''}"
  onanimationiteration={finishTurn}
>
  <Icon name="reload" />
</span>
