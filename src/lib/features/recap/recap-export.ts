import { t, type Language } from "../../i18n";
import { acquireGameArtwork } from "../../services/game-artwork";
import type { Game } from "../../services/local-state";
import { exportPlaytimeRecapImage, type PlaytimeRecap } from "../../services/playtime";
import { loadSteamImage } from "../../services/steam-details";
import { regionalLocale } from "../../utils/format";
import { formatPlaytime } from "../home/home-model";
import { readRecapCardColors, renderRecapCardPng, type RecapCardGame } from "./recap-card";
import { formatRecapPeriod, formatShare, recapFileName, type RecapSelection } from "./recap-model";
import { recapTiles } from "./recap-stats";

const cardTiles = ["total", "games", "streak", "day"];

function decodeImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("The image could not be decoded"));
    image.src = url;
  });
}

// Artwork URLs are in-memory blobs, so drawing them keeps the canvas exportable.
async function loadCover(game: Game | undefined): Promise<HTMLImageElement | null> {
  if (game === undefined) return null;
  try {
    const custom = acquireGameArtwork(game.id, "icon");
    try {
      const url = await custom.ready;
      if (url !== null) return await decodeImage(url);
    } finally {
      custom.release();
    }
    if (game.steamAppId === null) return null;
    const image = await loadSteamImage({ steamAppId: game.steamAppId, asset: "client_icon", fallbackAsset: null, index: null, full: false });
    return await decodeImage(image.url);
  } catch (error) {
    console.warn("Could not load a cover for the recap card", error);
    return null;
  }
}

// Returns false when the person closes the save dialog.
export async function exportRecapCard(selection: RecapSelection, recap: PlaytimeRecap, games: Game[], language: Language): Promise<boolean> {
  const locale = regionalLocale(language);
  const covers = await Promise.all(recap.topGames.map((entry) => loadCover(games.find((game) => game.id === entry.gameId))));
  const topGames: RecapCardGame[] = recap.topGames.map((entry, index) => ({
    name: entry.name,
    duration: formatPlaytime(entry.milliseconds),
    share: entry.share,
    shareLabel: formatShare(entry.share, locale),
    image: covers[index],
  }));
  const tiles = recapTiles(recap, locale, language).filter((tile) => cardTiles.includes(tile.key));
  const png = await renderRecapCardPng({
    brand: "Legio",
    heading: t("Annata di gioco", language),
    period: formatRecapPeriod(selection, locale),
    stats: cardTiles.flatMap((key) => tiles.filter((tile) => tile.key === key)).map((tile) => ({ label: tile.label, value: tile.value })),
    topTitle: t("Giochi più giocati", language),
    games: topGames,
    colors: readRecapCardColors(document.documentElement),
  });
  return exportPlaytimeRecapImage(png, recapFileName(selection));
}
