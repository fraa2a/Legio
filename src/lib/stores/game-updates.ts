import { listGameUpdates, type GameUpdate } from "../services/game-updates";
import { createResource } from "./resource";

export const gameUpdates = createResource<GameUpdate[]>([], listGameUpdates);
