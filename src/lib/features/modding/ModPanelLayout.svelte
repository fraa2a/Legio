<script lang="ts">
  import type { Snippet } from "svelte";
  import type { ModUpdate, ProfileMod } from "../../services/modding";
  import type { ModTabs } from "./mod-panel-model";
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
</script>

<!-- The list scrolls on its own so the mod panel beside it keeps its place. -->
<div class="relative flex min-h-0 flex-1 gap-4">
  <div class="flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-y-auto">
    {@render children()}
  </div>
  <ModPanel {ecosystemGameId} {tabs} {installed} {updates} {disabled} {openOnInstalled} {onChange} {onInstall} />
</div>
