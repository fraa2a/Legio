import { t } from "../i18n";

  export function mapToText(values: Record<string, string>): string {
    return Object.entries(values)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, value]) => `${key}=${value}`)
      .join("\n");
  }

  export function parseMap(value: string, label: string): Record<string, string> {
    const result: Record<string, string> = {};
    for (const line of value.split(/\r?\n/)) {
      if (line.trim().length === 0) continue;
      const separator = line.indexOf("=");
      const key = separator < 0 ? "" : line.slice(0, separator).trim();
      if (key.length === 0) throw new Error(t("{0}: ogni riga deve contenere una chiave e un valore separati da =.", undefined, [label]));
      if (Object.hasOwn(result, key)) throw new Error(t("{0}: la chiave {1} è ripetuta.", undefined, [label, key]));
      result[key] = line.slice(separator + 1);
    }
    return result;
  }

  export function parseEnvironment(value: string, debugLogging: boolean): Record<string, string> {
    const environment = parseMap(value, t("Ambiente", undefined));
    const typedKeys = [
      "WINEPREFIX",
      "STEAM_COMPAT_DATA_PATH",
      "STEAM_COMPAT_CLIENT_INSTALL_PATH",
      "WINEDLLOVERRIDES",
      "PROTON_USE_WINED3D",
      "PROTON_ENABLE_WAYLAND",
      "ENABLE_VK_LAYER_VALVE_steam_overlay_1",
      "SteamOverlayGameId",
    ];
    for (const key of Object.keys(environment)) {
      if (typedKeys.some((managed) => key.toLowerCase() === managed.toLowerCase())) {
        throw new Error(t("La variabile {0} è gestita dalle opzioni tipizzate di Legio.", undefined, [key]));
      }
      if (debugLogging && ["PROTON_LOG", "PROTON_LOG_DIR", "SteamGameId"].some((managed) => key.toLowerCase() === managed.toLowerCase())) {
        throw new Error(t("La variabile {0} è gestita dai log di debug.", undefined, [key]));
      }
    }
    return environment;
  }

