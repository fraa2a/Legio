export type ShortcutAction =
  | { type: "palette" }
  | { type: "section"; index: number }
  | { type: "search" }
  | { type: "play" }
  | { type: "help" };

export interface KeyInput {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
}

// Where the keystroke happened: typing in a field, on a control that Enter would activate,
// or behind an open dialog.
export interface ShortcutScope {
  editing: boolean;
  control: boolean;
  dialog: boolean;
}

// Ctrl+1 to Ctrl+4 follow the order of the main views in the sidebar.
export const sectionShortcutCount = 4;

const textInputTypes = new Set(["text", "search", "email", "url", "tel", "password", "number", "date", "time", "datetime-local", "month", "week"]);
const controlSelector = "button, a[href], summary, [role='button'], [role='link'], [role='tab'], [role='menuitem'], [role='option'], [role='checkbox'], [role='radio'], [role='switch']";

export function shortcutScope(target: EventTarget | null, dialogOpen: boolean): ShortcutScope {
  const element = target instanceof Element ? target : null;
  const input = element instanceof HTMLInputElement ? element : null;
  const editing = element !== null && (
    (input !== null && textInputTypes.has(input.type))
    || element instanceof HTMLTextAreaElement
    || element instanceof HTMLSelectElement
    || (element instanceof HTMLElement && element.isContentEditable === true)
  );
  const control = element !== null && !editing && (input !== null || element.closest(controlSelector) !== null);
  return { editing, control, dialog: dialogOpen };
}

// Ctrl+K works everywhere. The other shortcuts stay out of dialogs; the bare keys also stay out
// of fields and of controls that already use Enter.
export function matchShortcut(event: KeyInput, scope: ShortcutScope): ShortcutAction | null {
  const command = event.ctrlKey || event.metaKey;
  const key = event.key.toLowerCase();
  if (command && !event.altKey && key === "k") return { type: "palette" };
  if (scope.dialog) return null;
  if (command && !event.altKey) {
    if (key === "f") return { type: "search" };
    const index = /^[1-9]$/.test(key) ? Number(key) - 1 : -1;
    return index >= 0 && index < sectionShortcutCount ? { type: "section", index } : null;
  }
  if (command || event.altKey || scope.editing) return null;
  if (event.key === "?") return { type: "help" };
  if (event.key === "Enter" && !scope.control) return { type: "play" };
  return null;
}
