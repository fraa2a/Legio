<script lang="ts">
  import { onMount } from "svelte";
  import { appInfo } from "../../stores/app-info";
  import { activeSection, sectionLabels } from "../../stores/navigation";
  import {
    closeWindow,
    isWindowMaximized,
    minimizeWindow,
    onWindowResized,
    toggleMaximizeWindow,
  } from "../../services/window";
  import WindowControlButton from "../ui/WindowControlButton.svelte";

  let maximized = $state(false);
  let heading: HTMLElement | undefined = $state();
  const showMaximize = $derived($appInfo.data.desktopEnvironment !== "hyprland");
  const title = $derived(sectionLabels[$activeSection]);

  $effect(() => {
    void title;
    heading?.focus();
  });

  onMount(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    const refreshMaximized = () => {
      void isWindowMaximized()
        .then((value) => {
          if (!disposed) maximized = value;
        })
        .catch((reason) => console.error(reason));
    };

    refreshMaximized();
    void onWindowResized(refreshMaximized)
      .then((unlistenFn) => {
        if (disposed) unlistenFn();
        else unlisten = unlistenFn;
      })
      .catch((reason) => console.error(reason));

    return () => {
      disposed = true;
      unlisten?.();
    };
  });

  function handleMinimize() {
    minimizeWindow().catch((reason) => console.error(reason));
  }

  function handleClose() {
    closeWindow().catch((reason) => console.error(reason));
  }

  async function handleToggleMaximize() {
    try {
      await toggleMaximizeWindow();
      maximized = await isWindowMaximized();
    } catch (reason) {
      console.error(reason);
    }
  }
</script>

<div
  class="my-2 grid shrink-0 grid-cols-[1fr_auto_1fr] items-center rounded-2xl bg-zinc-900 p-2 light:bg-zinc-50"
  data-tauri-drag-region
>
  <h1
    bind:this={heading}
    data-page-heading
    tabindex="-1"
    class="col-start-2 truncate text-center text-base font-semibold text-zinc-200 select-none focus:outline-none light:text-zinc-800"
  >
    {title}
  </h1>
  <div class="col-start-3 flex items-center justify-end gap-1">
    <WindowControlButton label="Minimize" onClick={handleMinimize}>
      <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
        <path d="M19,13H5V11H19V13Z" />
      </svg>
    </WindowControlButton>
    {#if showMaximize}
      <WindowControlButton label={maximized ? "Restore" : "Maximize"} onClick={handleToggleMaximize}>
        {#if maximized}
          <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
            <path d="M4,8H8V4H20V16H16V20H4V8M16,8V14H18V6H10V8H16M6,12V18H14V12H6Z" />
          </svg>
        {:else}
          <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
            <path d="M4,4H20V20H4V4M6,8V18H18V8H6Z" />
          </svg>
        {/if}
      </WindowControlButton>
    {/if}
    <WindowControlButton label="Close" variant="danger" onClick={handleClose}>
      <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
        <path d="M13.46,12L19,17.54V19H17.54L12,13.46L6.46,19H5V17.54L10.54,12L5,6.46V5H6.46L12,10.54L17.54,5H19V6.46L13.46,12Z" />
      </svg>
    </WindowControlButton>
  </div>
</div>