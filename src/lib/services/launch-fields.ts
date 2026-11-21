import { t } from "../i18n";

  export function mapToText(values: Record<string, string>): string {
    return Object.entries(values)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, value]) => `${key}=${value}`)
      .join("\n");
  }

  export function parseMap(value: string, label: string): Record<string, string> {
    const result: Record<string, string> = Object.create(null);
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

