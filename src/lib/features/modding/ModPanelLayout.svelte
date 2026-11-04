<script lang="ts">
  import type { Snippet } from "svelte";
  import type { ModUpdate, ProfileMod } from "../../services/modding";
  import { modPanelState, type ModPanelState, type ModTabs } from "./mod-panel-model";
  import ModPanel from "./ModPanel.svelte";

  let {
    ecosystemGameId,
    tabs,
    installed,
    updates,
    disabled,
    openOnInstalled = false,
    onChange,
    onInstall,
    children,
  }: {
    ecosystemGameId: string;
    tabs: ModTabs;
    installed: ReadonlyMap<string, ProfileMod>;
    updates: ReadonlyMap<string, ModUpdate>;
    disabled: boolean;
    openOnInstalled?: boolean;
    onChange: (tabs: ModTabs) => void;
    onInstall: (fullName: string, version: string | null) => void;
    children: Snippet;
  } = $props();

  // Below lg the open panel overlays the list; the column only holds the expand control.
  const columns: Record<ModPanelState, string> = {
    none: "grid-cols-[minmax(0,1fr)_0px] gap-x-0",
    collapsed: "grid-cols-[minmax(0,1fr)_2rem] gap-x-0.5",
    open: "grid-cols-[minmax(0,1fr)_0px] gap-x-0 lg:grid-cols-[minmax(0,1fr)_28rem] lg:gap-x-0.5 xl:grid-cols-[minmax(0,1fr)_32rem]",
  };
  const panelState = $derived(modPanelState(tabs));
</script>

<!-- The list scrolls on its own so the mod panel beside it keeps its place. The column widths animate so the list resizes with the panel. -->
<div class="relative grid min-h-0 flex-1 transition-[grid-template-columns,column-gap] duration-200 ease-out motion-reduce:transition-none {columns[panelState]}">
  <div class="flex min-h-0 min-w-0 flex-col gap-4 overflow-y-auto">
    {@render children()}
  </div>
  <ModPanel {ecosystemGameId} {tabs} {installed} {updates} {disabled} {openOnInstalled} {onChange} {onInstall} />
</div>
