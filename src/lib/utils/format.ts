const byteUnits = ["KB", "MB", "GB", "TB"];

export function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  let size = value / 1024;
  let unit = 0;
  while (size >= 1024 && unit < byteUnits.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toFixed(1)} ${byteUnits[unit]}`;
}

export function formatDate(value: string): string {
  return new Date(value).toLocaleDateString("it-IT");
}

// The backend reports cache freshness as epoch seconds.
export function formatDateTime(epochSeconds: number): string {
  return new Date(epochSeconds * 1000).toLocaleString("it-IT", {
    dateStyle: "short",
    timeStyle: "short",
  });
}
