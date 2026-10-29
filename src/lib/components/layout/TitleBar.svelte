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
  import IconButton from "../ui/IconButton.svelte";
  import Icon from "../ui/Icon.svelte";
  import { addGameDialogOpen, libraryPortrait, libraryQuery, storeQuery } from "../../stores/library-ui";
  import RefreshIcon from "../ui/RefreshIcon.svelte";
  import { catalog, runCatalogSearch } from "../../stores/catalog";
  import { refreshSource, sourceBusy } from "../../stores/source";
  import { settings, updateSettings } from "../../stores/settings";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";

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
  const onboarding = $derived($settings.status === "ready" && !$settings.data.onboardingCompleted);
  const title = $derived(onboarding ? t("Configurazione iniziale", $language) : (detail?.name ?? t(sectionLabels[$activeSection], $language)));
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

<div class="my-1.5 grid shrink-0 grid-cols-[minmax(auto,1fr)_minmax(0,32rem)_minmax(auto,1fr)] items-center gap-2" data-tauri-drag-region>
  <div class="col-start-1 flex items-center">
    {#if libraryList}
      <div class="legio-glass flex h-11 items-center rounded-xl p-1">
        <IconButton size="lg" label={t("Aggiungi gioco", $language)} title={t("Aggiungi gioco", $language)} onClick={() => addGameDialogOpen.set(true)}>
          <Icon name="plus" />
        </IconButton>
        <IconButton
          size="lg"
          label={t("Copertine verticali", $language)}
          pressed={$libraryPortrait}
          title={$libraryPortrait ? t("Mostra copertine orizzontali", $language) : t("Mostra copertine verticali", $language)}
          disabled={savingLibraryView}
          onClick={toggleLibraryView}
        >
          <Icon name={$libraryPortrait ? "landscape" : "portrait"} />
        </IconButton>
      </div>
    {:else if detail !== null}
      <div class="legio-glass flex h-11 items-center rounded-xl p-1">
        <button
          type="button"
          class="flex h-9 items-center gap-1.5 rounded-lg px-3 text-sm text-zinc-400 transition-colors duration-200 hover:bg-white/10 hover:text-zinc-100 light:text-zinc-600 light:hover:bg-zinc-900/10 light:hover:text-zinc-900"
          onclick={navigateBack}
        >
          <Icon name="back" />
          {backLabel}
        </button>
      </div>
    {/if}
  </div>
  {#if searchList}
    <div class="legio-glass col-start-2 flex h-11 w-full min-w-0 max-w-lg justify-self-center items-center gap-2 rounded-xl px-3">
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
        <IconButton
          label={t("Aggiorna store", $language)}
          title={t("Aggiorna store", $language)}
          onClick={() => { void refreshSource(); void runCatalogSearch($storeQuery); }}
        >
          <RefreshIcon spinning={$sourceBusy || $catalog.status === "loading"} />
        </IconButton>
      {/if}
    </div>
  {:else}
    <div class="legio-glass col-start-2 flex h-11 min-w-0 max-w-full items-center justify-self-center rounded-xl px-4">
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
    <div class="legio-glass flex h-11 items-center gap-1 rounded-xl p-1">
      <IconButton size="lg" label={t("Riduci a icona", $language)} onClick={handleMinimize}>
        <Icon name="minimize" />
      </IconButton>
      {#if showMaximize}
        <IconButton size="lg" label={maximized ? t("Ripristina finestra", $language) : t("Massimizza", $language)} onClick={handleToggleMaximize}>
          <Icon name={maximized ? "restore" : "maximize"} />
        </IconButton>
      {/if}
      <IconButton size="lg" variant="danger" label={t("Chiudi", $language)} onClick={handleClose}>
        <Icon name="close" />
      </IconButton>
    </div>
  </div>
</div>
