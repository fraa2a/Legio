import { getPlaytimeRecap, getPlaytimeRecapYears, type PlaytimeRecap } from "../services/playtime";
import { recapBoundaries, type RecapSelection } from "../features/recap/recap-model";
import { createResource } from "./resource";

export const recapYears = createResource<number[]>([], getPlaytimeRecapYears, (years) => years.length === 0);

export function createRecapResource(selection: RecapSelection) {
  return createResource<PlaytimeRecap | null>(
    null,
    () => getPlaytimeRecap(selection.year, selection.mode === "month" ? selection.month : null, recapBoundaries(selection)),
    (recap) => recap === null || recap.totalMilliseconds === 0,
  );
}
