<script lang="ts">
  import { tick, untrack } from "svelte";
  import { cubicInOut } from "svelte/easing";
  import DownloadsView from "../../features/downloads/DownloadsView.svelte";
  import HomeView from "../../features/home/HomeView.svelte";
  import GameDetailView from "../../features/library/GameDetailView.svelte";
  import LibraryView from "../../features/library/LibraryView.svelte";
  import StoreView from "../../features/store/StoreView.svelte";
  import {
    activeSection,
    sections,
    selectedGameId,
    selectedStoreGame,
    type Section,
  } from "../../stores/navigation";
  import { reducedMotion } from "../../utils/motion";

  let container: HTMLElement | undefined = $state();
  let rendered = $state<Section[]>(["home"]);
  let frame: number | null = null;
  let finishMotion: (() => void) | null = null;
  let navigation = 0;

  const page = $derived(`${$activeSection}:${$selectedGameId ?? ""}:${$selectedStoreGame?.steamAppId ?? ""}`);

  function offsetOf(section: Section): number {
    if (container === undefined) return 0;
    const element = container.querySelector<HTMLElement>(`[data-section="${section}"]`);
    if (element === null) return 0;
    return element.getBoundingClientRect().top - container.getBoundingClientRect().top + container.scrollTop;
  }

  function rangeFor(target: Section): Section[] {
    const positions = [...rendered, target].map((section) => sections.indexOf(section));
    return sections.slice(Math.min(...positions), Math.max(...positions) + 1);
  }

  function stopMotion(): void {
    if (frame !== null) {
      cancelAnimationFrame(frame);
      frame = null;
    }
    if (finishMotion !== null) {
      const resolve = finishMotion;
      finishMotion = null;
      resolve();
    }
  }

  async function navigate(target: Section): Promise<void> {
    const scroll = container;
    if (scroll === undefined) return;
    const id = ++navigation;
    stopMotion();

    const range = rangeFor(target);
    const mounts = range.length !== rendered.length || range.some((section, index) => section !== rendered[index]);
    const anchor = mounts ? rendered[0] : null;
    const anchorTop = anchor === null ? 0 : offsetOf(anchor);
    if (mounts) rendered = range;
    await tick();
    if (id !== navigation) return;
    // Sections prepended above the anchor shift it, so keep the current view in place.
    if (anchor !== null) scroll.scrollTop += offsetOf(anchor) - anchorTop;

    const start = scroll.scrollTop;
    const distance = offsetOf(target) - start;
    if (reducedMotion || distance === 0) {
      scroll.scrollTop = start + distance;
    } else {
      const duration = Math.min(900, Math.max(300, Math.abs(distance) * 0.6));
      const beganAt = performance.now();
      await new Promise<void>((resolve) => {
        finishMotion = resolve;
        const step = (now: number): void => {
          const progress = Math.min(1, (now - beganAt) / duration);
          scroll.scrollTop = start + (offsetOf(target) - start) * cubicInOut(progress);
          if (progress < 1) {
            frame = requestAnimationFrame(step);
            return;
          }
          frame = null;
          finishMotion = null;
          resolve();
        };
        frame = requestAnimationFrame(step);
      });
      if (id !== navigation) return;
    }

    if (rendered.length !== 1 || rendered[0] !== target) {
      rendered = [target];
      await tick();
      if (id !== navigation) return;
    }
    scroll.scrollTop = 0;
  }

  $effect(() => {
    const target = $activeSection;
    if (page && container !== undefined) untrack(() => void navigate(target));
    return () => {
      navigation += 1;
      stopMotion();
    };
  });
</script>

<main
  bind:this={container}
  class="min-h-0 min-w-0 flex-1 overflow-y-auto rounded-xl scrollbar-none bg-black light:bg-zinc-50"
>
  {#each rendered as section (section)}
    <div data-section={section} class={section === "home" ? "h-full" : "min-h-full"}>
      {#if section === "home"}
        <HomeView />
      {:else if section === "library"}
        {#if $selectedGameId !== null}
          <GameDetailView />
        {:else}
          <LibraryView />
        {/if}
      {:else if section === "store"}
        <StoreView />
      {:else}
        <DownloadsView />
      {/if}
    </div>
  {/each}
</main>
