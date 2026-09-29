import { getPlaytimeSummaries, type PlaytimeSummary } from "../services/playtime";
import { createResource } from "./resource";

export const playtime = createResource<PlaytimeSummary[]>([], getPlaytimeSummaries);
