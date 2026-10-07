import { readable } from "svelte/store";
import { onWindowActivity } from "../services/window";

export const windowActive = readable(false, onWindowActivity);
