<script lang="ts">
  import { t, language } from "../../i18n";
  import { onMount } from "svelte";
  import { appInfo } from "../../stores/app-info";
  import { games } from "../../stores/games";
  import {
    activeSection,
    closeGame,
    closeModManager,
    closeStoreGame,
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
  import Button from "../ui/Button.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import Icon from "../ui/Icon.svelte";
  import { addGameDialogOpen, libraryPortrait, libraryQuery, storeQuery } from "../../stores/library-ui";
  import RefreshIcon from "../ui/RefreshIcon.svelte";
  import TextField from "../ui/TextField.svelte";
  import { catalog, runCatalogSearch } from "../../stores/catalog";
  import { refreshSource, sourceBusy } from "../../stores/source";
  import { settings, updateSettings } from "../../stores/settings";
  import { showToast } from "../../stores/toast";
  import { toMessage } from "../../utils/errors";
  import { titleBarContent } from "../../stores/title-bar";

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
  const game = $derived(
    $activeSection === "library" && $selectedGameId !== null
      ? ($games.data.find((entry) => entry.id === $selectedGameId) ?? null)
      : null,
  );
  const storeGame = $derived($activeSection === "store" ? $selectedStoreGame : null);
  const modSelection = $derived($activeSection === "mods" ? $selectedModGame : null);
  const modGameName = $derived(modSelection === null ? "" : ($games.data.find((entry) => entry.id === modSelection.gameId)?.name ?? ""));
  const detail = $derived(storeGame ?? game ?? (modSelection === null ? null : { name: t("Mod di {0}", $language, [modGameName]) }));
  const backLabel = $derived.by(() => {
    if (storeGame !== null) return t("Risultati", $language);
    if (modSelection !== null) return modSelection.returnTo === "game" ? modGameName : t("Mod", $language);
    return t("Libreria", $language);
  });
  const onboarding = $derived($settings.status === "ready" && !$settings.data.onboardingCompleted);
  const pageTitle = $derived(onboarding ? t("Configurazione iniziale", $language) : (detail?.name ?? t(sectionLabels[$activeSection], $language)));
  const pageContent = $derived(onboarding ? null : $titleBarContent);
  const title = $derived(pageContent?.title ?? pageTitle);
  const libraryList = $derived($activeSection === "library" && $selectedGameId === null);
  const storeList = $derived($activeSection === "store" && $selectedStoreGame === null);
  const searchList = $derived(libraryList || storeList);

  // Page-provided titles change with page state, so they do not move the focus.
  $effect(() => {
    void pageTitle;
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
    else if (modSelection !== null) closeModManager();
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
  <div class="col-start-1 flex min-w-0 items-center gap-2">
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
    {:else if detail !== null}
      <div class="legio-glass flex h-control-md items-center rounded-card p-1">
        <Button label={backLabel} variant="ghost" size="sm" onClick={navigateBack}>
          <Icon name="back" />
        </Button>
      </div>
    {/if}
    {#if pageContent?.start}{@render pageContent.start()}{/if}
  </div>
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
      class="col-start-2 w-full min-w-0 max-w-lg justify-self-center"
      label={libraryList ? t("Cerca nella libreria", $language) : t("Cerca nello store", $language)}
      labelHidden
      bind:element={searchInput}
      value={libraryList ? $libraryQuery : $storeQuery}
      placeholder={libraryList ? t("Cerca nella libreria", $language) : t("Cerca nello store", $language)}
      oninput={(value) => libraryList ? libraryQuery.set(value) : storeQuery.set(value)}
      trailing={storeList ? refreshStore : undefined}
    />
  {:else}
    <div class="legio-glass col-start-2 flex h-control-md min-w-0 max-w-full items-center justify-self-center rounded-card px-4">
      <h1
        bind:this={heading}
        data-page-heading
        tabindex="-1"
        class="w-full truncate text-center text-lg font-semibold text-legio-fg select-none focus:outline-none"
      >
        {title}
      </h1>
    </div>
  {/if}
  <div class="col-start-3 flex min-w-0 items-center justify-end gap-2">
    {#if pageContent?.end}{@render pageContent.end()}{/if}
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
