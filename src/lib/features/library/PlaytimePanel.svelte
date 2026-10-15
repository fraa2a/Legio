<script lang="ts">
  import type { Game } from "../../services/local-state";
  import { setGamePlaytime } from "../../services/playtime";
  import { playtime } from "../../stores/playtime";
  import { launchStateByGame } from "../../stores/launch";
  import { language, t } from "../../i18n";
  import { toMessage } from "../../utils/errors";
  import Panel from "../../components/ui/Panel.svelte";
  import TextField from "../../components/ui/TextField.svelte";
  import Button from "../../components/ui/Button.svelte";
  import ErrorBanner from "../../components/ui/ErrorBanner.svelte";

  let { game }: { game: Game } = $props();
  let hoursDraft = $state<string | null>(null);
  let minutesDraft = $state<string | null>(null);
  let confirming = $state(false);
  let pending = $state(false);
  let error = $state<string | null>(null);
  let saved = $state(false);
  const summary = $derived($playtime.data.find((entry) => entry.gameId === game.id));
  const total = $derived(summary?.totalMilliseconds ?? 0);
  const hours = $derived(hoursDraft ?? String(Math.floor(total / 3_600_000)));
  const minutes = $derived(minutesDraft ?? String(Math.floor(total / 60_000) % 60));
  const requested = $derived(Number(hours) * 3_600_000 + Number(minutes) * 60_000);
  const valid = $derived(/^\d+$/.test(hours) && /^\d+$/.test(minutes) && Number(minutes) < 60 && Number.isSafeInteger(requested) && requested >= 0);
  const running = $derived(($launchStateByGame.get(game.id)?.status ?? "idle") !== "idle" || (summary?.activeSessions ?? 0) > 0);
  $effect(() => { void playtime.load(); });

  function edit(which: "hours" | "minutes", value: string): void {
    if (which === "hours") hoursDraft = value;
    else minutesDraft = value;
    confirming = false;
    saved = false;
  }

  async function save(): Promise<void> {
    if (!valid || running || pending) return;
    pending = true;
    error = null;
    saved = false;
    try {
      await setGamePlaytime(game.id, requested);
      await playtime.load();
      hoursDraft = null;
      minutesDraft = null;
      confirming = false;
      saved = true;
    } catch (cause) { error = toMessage(cause); }
    finally { pending = false; }
  }
</script>

<Panel title={t("Modifica tempo di gioco", $language)}>
  <p class="text-sm text-amber-400 light:text-amber-700">{t("Impostazione avanzata: sostituisce il totale locale di Legio. Non modifica Steam né la cronologia delle sessioni.", $language)}</p>
  <div class="grid grid-cols-2 gap-3">
    <TextField id="playtime-hours" label={t("Ore", $language)} value={hours} inputmode="numeric" disabled={pending || running} oninput={(value) => edit("hours", value)} />
    <TextField id="playtime-minutes" label={t("Minuti", $language)} value={minutes} inputmode="numeric" disabled={pending || running} oninput={(value) => edit("minutes", value)} />
  </div>
  {#if running}<p class="text-sm text-zinc-400 light:text-zinc-600">{t("Chiudi il gioco prima di modificare il tempo di gioco.", $language)}</p>{/if}
  {#if !valid}<p class="text-sm text-amber-400 light:text-amber-700">{t("Inserisci ore intere non negative e minuti da 0 a 59.", $language)}</p>{/if}
  <div class="flex flex-wrap gap-2">
    {#if confirming}
      <Button label={t("Annulla", $language)} variant="secondary" disabled={pending} onClick={() => (confirming = false)} />
      <Button label={pending ? t("Salvataggio...", $language) : t("Conferma modifica", $language)} variant="danger" disabled={pending || running || !valid} onClick={() => void save()} />
    {:else}
      <Button label={t("Imposta tempo di gioco", $language)} variant="danger" disabled={pending || running || !valid || summary === undefined} onClick={() => (confirming = true)} />
    {/if}
  </div>
  {#if error}<ErrorBanner message={error} />{/if}
  {#if saved}<p class="text-sm text-emerald-400 light:text-emerald-700" role="status">{t("Tempo di gioco aggiornato.", $language)}</p>{/if}
</Panel>
