<script lang="ts">
  import { t, language } from "../../i18n";
  import { onMount } from "svelte";
  import { appInfo } from "../../stores/app-info";
  import { games } from "../../stores/games";
  import {
    activeSection,
    closeGame,
    closeStoreGame,
    sectionLabels,
    selectedGameId,
    selectedStoreGame,
  } from "../../stores/navigation";
  import {
    closeWindow,
    isWindowMaximized,
    minimizeWindow,
    onWindowResized,
    toggleMaximizeWindow,
  } from "../../services/window";
  import WindowControlButton from "../ui/WindowControlButton.svelte";
  import Icon from "../ui/Icon.svelte";
  import { addGameDialogOpen, libraryPortrait, libraryQuery, storeQuery } from "../../stores/library-ui";
  import { runCatalogSearch } from "../../stores/catalog";
  import { refreshSource } from "../../stores/source";

  let maximized = $state(false);
  let heading: HTMLElement | undefined = $state();
  let searchInput: HTMLInputElement | undefined = $state();
  const showMaximize = $derived($appInfo.data.desktopEnvironment !== "hyprland");
  const game = $derived(
    $activeSection === "library" && $selectedGameId !== null
      ? ($games.data.find((entry) => entry.id === $selectedGameId) ?? null)
      : null,
  );
  const storeGame = $derived($activeSection === "store" ? $selectedStoreGame : null);
  const detail = $derived(storeGame ?? game);
  const backLabel = $derived(storeGame !== null ? t("Risultati", $language) : t("Libreria", $language));
  const title = $derived(detail?.name ?? t(sectionLabels[$activeSection], $language));
  const libraryList = $derived($activeSection === "library" && $selectedGameId === null);
  const storeList = $derived($activeSection === "store" && $selectedStoreGame === null);
  const searchList = $derived(libraryList || storeList);

  $effect(() => {
    void title;
    if (searchList) searchInput?.focus();
    else heading?.focus();
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

  function navigateBack(): void {
    if (storeGame !== null) closeStoreGame();
    else closeGame();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key !== "Escape" || event.defaultPrevented || detail === null) return;
    if (document.querySelector("dialog[open], [aria-modal='true']") !== null) return;
    navigateBack();
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="my-1.5 grid shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-2" data-tauri-drag-region>
  <div class="col-start-1 flex items-center">
    {#if libraryList}
      <div class="flex h-11 items-center rounded-xl bg-zinc-900 p-1 light:bg-zinc-50">
        <button
          type="button"
          aria-label={t("Aggiungi gioco", $language)}
          title={t("Aggiungi gioco", $language)}
          class="flex size-9 items-center justify-center rounded-xl text-zinc-300 transition-colors hover:bg-white/10 hover:text-white light:text-zinc-700 light:hover:bg-zinc-900/10 light:hover:text-zinc-900"
          onclick={() => addGameDialogOpen.set(true)}
        >
          <Icon name="plus" />
        </button>
        <button
          type="button"
          aria-label={t("Copertine verticali", $language)}
          aria-pressed={$libraryPortrait}
          title={$libraryPortrait ? t("Mostra copertine orizzontali", $language) : t("Mostra copertine verticali", $language)}
          class="flex size-9 items-center justify-center rounded-xl transition-colors hover:bg-white/10 hover:text-white light:hover:bg-zinc-900/10 light:hover:text-zinc-900 {$libraryPortrait ? 'bg-white/10 text-white light:bg-zinc-900/10 light:text-zinc-900' : 'text-zinc-300 light:text-zinc-700'}"
          onclick={() => libraryPortrait.update((portrait) => !portrait)}
        >
          <Icon name={$libraryPortrait ? "landscape" : "portrait"} />
        </button>
      </div>
    {:else if detail !== null}
      <div class="flex h-11 items-center rounded-xl bg-zinc-900 p-1 light:bg-zinc-50">
        <button
          type="button"
          class="flex h-9 items-center gap-1.5 rounded-xl px-3 text-sm text-zinc-400 transition-colors duration-200 hover:bg-white/10 hover:text-zinc-100 light:hover:bg-zinc-900/10 light:hover:text-zinc-900"
          onclick={navigateBack}
        >
          <svg class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2Z" />
          </svg>
          {backLabel}
        </button>
      </div>
    {/if}
  </div>
  {#if searchList}
    <div class="col-start-2 flex h-11 w-[min(32rem,calc(100vw-24rem))] min-w-0 items-center gap-2 rounded-xl bg-zinc-900 px-3 light:bg-zinc-50">
      <Icon name="search" size="h-4 w-4 shrink-0 text-zinc-500" />
      <input
        bind:this={searchInput}
        value={libraryList ? $libraryQuery : $storeQuery}
        type="search"
        aria-label={libraryList ? t("Cerca nella libreria", $language) : t("Cerca nello store", $language)}
        placeholder={libraryList ? t("Cerca nella libreria", $language) : t("Cerca nello store", $language)}
        oninput={(event) => libraryList ? libraryQuery.set(event.currentTarget.value) : storeQuery.set(event.currentTarget.value)}
        class="h-full min-w-0 flex-1 bg-transparent text-sm text-zinc-100 outline-none placeholder:text-zinc-500 light:text-zinc-900"
      />
      {#if storeList}
        <button
          type="button"
          class="flex size-8 shrink-0 items-center justify-center rounded-lg text-zinc-400 transition-colors hover:bg-white/10 hover:text-zinc-100 light:text-zinc-600 light:hover:bg-zinc-900/10 light:hover:text-zinc-900"
          aria-label={t("Aggiorna store", $language)}
          title={t("Aggiorna store", $language)}
          onclick={() => { void refreshSource(); void runCatalogSearch($storeQuery); }}
        >
          <Icon name="reload" size="h-4 w-4" />
        </button>
      {/if}
    </div>
  {:else}
    <div class="col-start-2 flex h-11 items-center rounded-xl bg-zinc-900 px-4 light:bg-zinc-50">
      <h1
        bind:this={heading}
        data-page-heading
        tabindex="-1"
        class="w-full truncate text-center text-lg font-semibold text-zinc-200 select-none focus:outline-none light:text-zinc-800"
      >
        {title}
      </h1>
    </div>
  {/if}
  <div class="col-start-3 flex items-center justify-end">
    <div class="flex h-11 items-center gap-1 rounded-xl bg-zinc-900 p-1 light:bg-zinc-50">
      <WindowControlButton label="Minimize" onClick={handleMinimize}>
        <svg class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
          <path d="M19,13H5V11H19V13Z" />
        </svg>
      </WindowControlButton>
      {#if showMaximize}
        <WindowControlButton label={maximized ? "Restore" : "Maximize"} onClick={handleToggleMaximize}>
          {#if maximized}
            <svg class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
              <path d="M4,8H8V4H20V16H16V20H4V8M16,8V14H18V6H10V8H16M6,12V18H14V12H6Z" />
            </svg>
          {:else}
            <svg class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
              <path d="M4,4H20V20H4V4M6,8V18H18V8H6Z" />
            </svg>
          {/if}
        </WindowControlButton>
      {/if}
      <WindowControlButton label="Close" variant="danger" onClick={handleClose}>
        <svg class="h-5 w-5" viewBox="0 0 24 24" fill="currentColor">
          <path d="M13.46,12L19,17.54V19H17.54L12,13.46L6.46,19H5V17.54L10.54,12L5,6.46V5H6.46L12,10.54L17.54,5H19V6.46L13.46,12Z" />
        </svg>
      </WindowControlButton>
    </div>
  </div>
</div>
