<script lang="ts">
  import { t, language } from "../../i18n";
  import { onMount } from "svelte";
  import { appInfo } from "../../stores/app-info";
  import { games } from "../../stores/games";
  import {
    activeSection,
    closeGame,
    closeModManager,
    closePlaytimeRecap,
    closeStoreGame,
    playtimeRecapOpen,
    sectionLabels,
    selectedGameId,
    selectedModGame,
    selectedStoreGame,
  } from "../../stores/navigation";
  import {
    closeWindow,
    isWindowMaximized,
    minimizeWindow,
    onWindowResized,
    toggleMaximizeWindow,
  } from "../../services/window";
  import IconButton from "../ui/IconButton.svelte";
  import Icon from "../ui/Icon.svelte";
  import { addGameDialogOpen, libraryPortrait, libraryQuery, storeQuery } from "../../stores/library-ui";
  import RefreshIcon from "../ui/RefreshIcon.svelte";
  import TextField from "../ui/TextField.svelte";
  import { catalog, runCatalogSearch } from "../../stores/catalog";
  import { openPalette } from "../../stores/command-palette";
  import { refreshSource, sourceBusy } from "../../stores/source";
  import { settings, updateSettings } from "../../stores/settings";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import { titleBarContent } from "../../stores/title-bar";
  import NotificationCenter from "../../features/notifications/NotificationCenter.svelte";
  import TitleBarDownloadStatus from "./TitleBarDownloadStatus.svelte";
  import { titleBarPath } from "./title-bar-model";

  let savingLibraryView = $state(false);

  async function toggleLibraryView(): Promise<void> {
    savingLibraryView = true;
    try {
      await updateSettings({ libraryPortrait: !$libraryPortrait });
    } catch (error) {
      showToast(toMessage(error), "error");
    } finally {
      savingLibraryView = false;
    }
  }

  let maximized = $state(false);
  let heading: HTMLElement | undefined = $state();
  let searchInput: HTMLInputElement | HTMLTextAreaElement | undefined = $state();
  const showMaximize = $derived($appInfo.data.desktopEnvironment !== "hyprland");
  const onboarding = $derived($settings.status === "ready" && !$settings.data.onboardingCompleted);
  const path = $derived(onboarding
    ? { back: null, current: t("Configurazione iniziale", $language) }
    : titleBarPath(
      { section: $activeSection, gameId: $selectedGameId, storeGame: $selectedStoreGame, modGame: $selectedModGame, recapOpen: $playtimeRecapOpen },
      (id) => $games.data.find((entry) => entry.id === id)?.name ?? null,
      (section) => t(sectionLabels[section], $language),
      t("Annata di gioco", $language),
    ));
  const pageContent = $derived(onboarding ? null : $titleBarContent);
  const libraryList = $derived($activeSection === "library" && $selectedGameId === null);
  const storeList = $derived($activeSection === "store" && $selectedStoreGame === null);
  // These views filter their own list from the title bar; the others search everywhere through the palette.
  const searchList = $derived(!onboarding && (libraryList || storeList));

  $effect(() => {
    void path.current;
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
    if ($activeSection === "home" && $playtimeRecapOpen) closePlaytimeRecap();
    else if ($activeSection === "store" && $selectedStoreGame !== null) closeStoreGame();
    else if ($activeSection === "mods" && $selectedModGame !== null) closeModManager();
    else closeGame();
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key !== "Escape" || event.defaultPrevented || path.back === null) return;
    if (document.querySelector("dialog[open], [aria-modal='true']") !== null) return;
    navigateBack();
  }

  // Typing on the search button starts the palette query with that key.
  function handleSearchKeydown(event: KeyboardEvent): void {
    if (event.ctrlKey || event.metaKey || event.altKey || event.isComposing) return;
    if (event.key.length === 1 && event.key !== " ") {
      event.preventDefault();
      openPalette(event.key);
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      openPalette();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- Only elements carrying data-tauri-drag-region drag the window, so every non-interactive wrapper has it.
     When space runs out the search narrows first, then the path truncates; the right side never shrinks. -->
<div class="@container my-1.5 flex shrink-0 items-center gap-2" data-tauri-drag-region>
  <div class="flex min-w-0 items-center gap-2" data-tauri-drag-region>
    <nav aria-label={t("Percorso", $language)} class="legio-glass flex h-control-md min-w-0 items-center rounded-card px-1" data-tauri-drag-region>
      <ol class="flex min-w-0 items-center gap-1 text-sm" data-tauri-drag-region>
        {#if path.back !== null}
          <li class="flex min-w-0 items-center gap-1" data-tauri-drag-region>
            <button
              type="button"
              title={path.back}
              aria-label={t("Torna a {0}", $language, [path.back])}
              onclick={navigateBack}
              class="flex h-control-sm max-w-48 min-w-0 items-center gap-1.5 rounded-control px-2 font-medium text-legio-fg-muted transition-colors hover:bg-legio-fill-hover hover:text-legio-fg"
            >
              <span class="shrink-0"><Icon name="back" size="sm" /></span>
              <span class="truncate">{path.back}</span>
            </button>
            <span class="text-legio-fg-subtle" aria-hidden="true" data-tauri-drag-region>/</span>
          </li>
        {/if}
        <li class="flex min-w-0 items-center" data-tauri-drag-region>
          <h1
            bind:this={heading}
            data-page-heading
            data-tauri-drag-region
            tabindex="-1"
            aria-current="page"
            title={path.current}
            class="max-w-80 truncate px-2 text-sm font-semibold text-legio-fg focus:outline-none"
          >
            {path.current}
          </h1>
        </li>
      </ol>
    </nav>
    {#if libraryList}
      <div class="legio-glass flex h-control-md items-center rounded-card p-1">
        <IconButton label={t("Aggiungi gioco", $language)} title={t("Aggiungi gioco", $language)} onClick={() => addGameDialogOpen.set(true)}>
          <Icon name="plus" />
        </IconButton>
        <IconButton
          label={t("Copertine verticali", $language)}
          pressed={$libraryPortrait}
          title={$libraryPortrait ? t("Mostra copertine orizzontali", $language) : t("Mostra copertine verticali", $language)}
          disabled={savingLibraryView}
          onClick={toggleLibraryView}
        >
          <Icon name={$libraryPortrait ? "landscape" : "portrait"} />
        </IconButton>
      </div>
    {/if}
    {#if pageContent?.start}{@render pageContent.start()}{/if}
  </div>
  <!-- Under 12rem the palette button keeps only its icon. -->
  <div class="@container flex min-w-control-md flex-1 justify-center" data-tauri-drag-region>
    {#if searchList}
      {#snippet refreshStore()}
        <IconButton
          label={t("Aggiorna store", $language)}
          title={t("Aggiorna store", $language)}
          onClick={() => { void refreshSource(); void runCatalogSearch($storeQuery); }}
        >
          <RefreshIcon spinning={$sourceBusy || $catalog.status === "loading"} />
        </IconButton>
      {/snippet}
      <TextField
        id="title-bar-search"
        type="search"
        variant="toolbar"
        class="w-full max-w-md min-w-0"
        label={libraryList ? t("Cerca nella libreria", $language) : t("Cerca nello store", $language)}
        labelHidden
        bind:element={searchInput}
        value={libraryList ? $libraryQuery : $storeQuery}
        placeholder={libraryList ? t("Cerca nella libreria", $language) : t("Cerca nello store", $language)}
        oninput={(value) => libraryList ? libraryQuery.set(value) : storeQuery.set(value)}
        trailing={storeList ? refreshStore : undefined}
      />
    {:else if !onboarding}
      <button
        id="title-bar-palette"
        type="button"
        aria-label={t("Cerca ovunque", $language)}
        aria-haspopup="dialog"
        aria-keyshortcuts="Control+K"
        onclick={() => openPalette()}
        onkeydown={handleSearchKeydown}
        class="legio-glass flex h-control-md w-full max-w-md min-w-0 items-center gap-2.5 rounded-card px-3 text-sm text-legio-fg-subtle transition-colors hover:text-legio-fg-muted"
      >
        <span class="shrink-0"><Icon name="search" /></span>
        <span class="hidden min-w-0 flex-1 truncate text-left @min-[12rem]:inline">{t("Cerca ovunque", $language)}</span>
        <kbd class="hidden shrink-0 text-xs font-medium @min-[18rem]:inline">Ctrl K</kbd>
      </button>
    {/if}
  </div>
  <div class="flex shrink-0 items-center gap-2" data-tauri-drag-region>
    {#if pageContent?.end}{@render pageContent.end()}{/if}
    {#if !onboarding}
      <TitleBarDownloadStatus />
      <NotificationCenter />
    {/if}
    <div class="legio-glass flex h-control-md items-center gap-1 rounded-card p-1">
      <IconButton label={t("Riduci a icona", $language)} onClick={handleMinimize}>
        <Icon name="minimize" />
      </IconButton>
      {#if showMaximize}
        <IconButton label={maximized ? t("Ripristina finestra", $language) : t("Massimizza", $language)} onClick={handleToggleMaximize}>
          <Icon name={maximized ? "restore" : "maximize"} />
        </IconButton>
      {/if}
      <IconButton variant="danger" label={t("Chiudi", $language)} onClick={handleClose}>
        <Icon name="close" />
      </IconButton>
    </div>
  </div>
</div>
