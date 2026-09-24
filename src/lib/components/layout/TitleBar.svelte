<script lang="ts">
  import { onMount } from "svelte";
  import { getAppInfo } from "../../services/app";
  import {
    closeWindow,
    isWindowMaximized,
    minimizeWindow,
    onWindowResized,
    toggleMaximizeWindow,
  } from "../../services/window";
  import WindowControlButton from "../ui/WindowControlButton.svelte";

  let maximized = $state(false);
  let isLinux = $state(false);

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

    void getAppInfo()
      .then((info) => {
        if (!disposed) isLinux = info.platform === "linux";
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

<div class="absolute inset-x-0 top-0 h-6" data-tauri-drag-region></div>
<div class="absolute right-8 top-8 flex items-center gap-1 rounded-full bg-white/5 p-1">
  <WindowControlButton label="Minimize" onClick={handleMinimize}>
    <svg class="h-4 w-4" viewBox="0 0 24 24" fill="currentColor">
      <path d="M19,13H5V11H19V13Z" />
    </svg>
  </WindowControlButton>
  {#if !isLinux}
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