<script lang="ts">
  import { onDestroy } from "svelte";
  import { toast } from "../../stores/toast";

  let notice: HTMLDivElement | undefined = $state();
  let frame = 0;
  let hideTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => {
    if (!notice) return;
    cancelAnimationFrame(frame);
    if (hideTimer !== undefined) clearTimeout(hideTimer);

    const current = $toast;
    if (current) {
      if (!notice.matches(":popover-open")) notice.showPopover();
      frame = requestAnimationFrame(() => {
        frame = requestAnimationFrame(() => {
          if ($toast?.id === current.id) notice?.setAttribute("data-visible", "true");
        });
      });
    } else if (notice.matches(":popover-open")) {
      notice.setAttribute("data-visible", "false");
      hideTimer = setTimeout(() => notice?.hidePopover(), 180);
    }
  });

  onDestroy(() => {
    cancelAnimationFrame(frame);
    if (hideTimer !== undefined) clearTimeout(hideTimer);
  });
</script>

<div
  bind:this={notice}
  popover="manual"
  class="legio-toast max-w-sm rounded-xl border border-white/15 bg-zinc-900/95 px-4 py-3 text-sm text-emerald-300 shadow-xl light:border-zinc-900/15 light:bg-white light:text-emerald-700"
  role="status"
  aria-live="polite"
>
  {$toast?.message ?? ""}
</div>
