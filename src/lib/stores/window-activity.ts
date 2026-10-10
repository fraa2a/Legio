import { readable } from "svelte/store";
import { onWindowActivity } from "../services/window";

export const windowActive = readable(false, (listener) => onWindowActivity(listener));

export const windowVisible = readable(false, (listener) => onWindowActivity(listener, { requireFocus: false }));
