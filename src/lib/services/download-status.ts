export type StatusTone = "neutral" | "info" | "success" | "danger" | "warning";
export type ProgressTone = "default" | "accent" | "warning" | "danger" | "success";
interface DownloadState {
  description: string;
  badge: string;
  phase: string | null;
  tone: StatusTone;
  progress: ProgressTone;
  section: "processing" | "queue" | "finished";
  rank: number;
  active: boolean;
}

const downloadStates = {
  queued: {
    description: "in coda",
    badge: "In coda",
    phase: null,
    tone: "info",
    progress: "default",
    section: "queue",
    rank: 1,
    active: true,
  },
  downloading: {
    description: "download in corso",
    badge: "Scaricamento",
    phase: null,
    tone: "info",
    progress: "accent",
    section: "processing",
    rank: 0,
    active: true,
  },
  waiting: {
    description: "in attesa",
    badge: "In attesa",
    phase: "In attesa di rete",
    tone: "info",
    progress: "default",
    section: "queue",
    rank: 1,
    active: true,
  },
  paused: {
    description: "download in pausa",
    badge: "In pausa",
    phase: "Download in pausa",
    tone: "warning",
    progress: "warning",
    section: "queue",
    rank: 2,
    active: false,
  },
  failed: {
    description: "download fallito",
    badge: "Errore",
    phase: "Download interrotto",
    tone: "danger",
    progress: "danger",
    section: "queue",
    rank: 3,
    active: false,
  },
  downloaded: {
    description: "download completato",
    badge: "Verifica",
    phase: "Verifica del file",
    tone: "info",
    progress: "accent",
    section: "processing",
    rank: 0,
    active: true,
  },
  staging: {
    description: "estrazione in corso",
    badge: "Estrazione",
    phase: "Estrazione dei file",
    tone: "info",
    progress: "accent",
    section: "processing",
    rank: 0,
    active: true,
  },
  staged: {
    description: "estrazione completata",
    badge: "Pronto",
    phase: "Installazione automatica in attesa",
    tone: "info",
    progress: "accent",
    section: "processing",
    rank: 4,
    active: true,
  },
  finalizing: {
    description: "installazione in corso",
    badge: "Installazione",
    phase: "Installazione in corso",
    tone: "info",
    progress: "accent",
    section: "processing",
    rank: 0,
    active: true,
  },
  installed: {
    description: "installato",
    badge: "Completato",
    phase: "Installazione completata",
    tone: "success",
    progress: "success",
    section: "finished",
    rank: 5,
    active: false,
  },
  cancelled: {
    description: "download annullato",
    badge: "Annullato",
    phase: "Download annullato",
    tone: "neutral",
    progress: "default",
    section: "finished",
    rank: 6,
    active: false,
  },
} satisfies Record<string, DownloadState>;

export type DownloadStatus = keyof typeof downloadStates;

export function downloadState(status: string): DownloadState | undefined {
  return Object.hasOwn(downloadStates, status) ? downloadStates[status as DownloadStatus] : undefined;
}
