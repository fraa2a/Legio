export interface RecapSelection {
  mode: "year" | "month";
  year: number;
  // 1 to 12.
  month: number;
}

export interface ChartBar {
  label: string;
  value: number;
  // 0 to 100 of the tallest bar; nonzero values keep a visible sliver.
  percent: number;
  text: string;
}

// Local midnights around every day of the period, as the backend expects them.
export function recapBoundaries(selection: RecapSelection): number[] {
  const first = selection.mode === "year" ? new Date(selection.year, 0, 1) : new Date(selection.year, selection.month - 1, 1);
  const end = selection.mode === "year" ? new Date(selection.year + 1, 0, 1) : new Date(selection.year, selection.month, 1);
  const boundaries: number[] = [];
  for (let day = 0; ; day++) {
    const boundary = new Date(first.getFullYear(), first.getMonth(), first.getDate() + day);
    boundaries.push(boundary.getTime());
    if (boundary.getTime() >= end.getTime()) return boundaries;
  }
}

// The current year when it has data, otherwise the newest year that does.
export function defaultRecapSelection(years: readonly number[], now: Date): RecapSelection {
  const year = years.includes(now.getFullYear()) ? now.getFullYear() : (years[0] ?? now.getFullYear());
  return { mode: "year", year, month: year === now.getFullYear() ? now.getMonth() + 1 : 12 };
}

export function recapFileName(selection: RecapSelection): string {
  return selection.mode === "year" ? `legio-recap-${selection.year}.png` : `legio-recap-${selection.year}-${String(selection.month).padStart(2, "0")}.png`;
}

function capitalize(text: string, locale: string): string {
  return text.charAt(0).toLocaleUpperCase(locale) + text.slice(1);
}

export function formatRecapPeriod(selection: RecapSelection, locale: string): string {
  if (selection.mode === "year") return String(selection.year);
  return capitalize(new Date(selection.year, selection.month - 1, 1).toLocaleDateString(locale, { month: "long", year: "numeric" }), locale);
}

export function monthNames(locale: string, month: "long" | "short"): string[] {
  return Array.from({ length: 12 }, (_, index) => capitalize(new Date(2024, index, 1).toLocaleDateString(locale, { month }), locale));
}

// Monday first, like the backend distribution.
export function weekdayNames(locale: string, weekday: "long" | "short"): string[] {
  return Array.from({ length: 7 }, (_, index) => capitalize(new Date(2024, 0, 1 + index).toLocaleDateString(locale, { weekday }), locale));
}

// Dates arrive as local calendar days (YYYY-MM-DD), so they must not pass through UTC.
export function formatRecapDate(isoDate: string, locale: string, options: Intl.DateTimeFormatOptions = { day: "numeric", month: "long", year: "numeric" }): string {
  const [year, month, day] = isoDate.split("-").map(Number);
  return new Date(year, month - 1, day).toLocaleDateString(locale, options);
}

export function formatShare(share: number, locale: string): string {
  const percent = share * 100;
  if (percent > 0 && percent < 1) return `<${new Intl.NumberFormat(locale, { style: "percent", maximumFractionDigits: 0 }).format(0.01)}`;
  return new Intl.NumberFormat(locale, { style: "percent", maximumFractionDigits: 0 }).format(share);
}

export function chartBars(values: readonly number[], labels: readonly string[], format: (milliseconds: number) => string): ChartBar[] {
  const peak = Math.max(0, ...values);
  return values.map((value, index) => ({
    label: labels[index] ?? "",
    value,
    percent: value <= 0 || peak <= 0 ? 0 : Math.max(2, (value / peak) * 100),
    text: `${labels[index] ?? ""}: ${format(value)}`,
  }));
}

// Day labels of a month chart: every fifth day plus the first, so they never crowd.
export function dayLabels(days: number): string[] {
  return Array.from({ length: days }, (_, index) => String(index + 1));
}

export function showsDayLabel(day: number, days: number): boolean {
  return day === 1 || (day % 5 === 0 && days - day >= 2);
}

export function monthOptions(locale: string): { value: string; label: string }[] {
  return monthNames(locale, "long").map((label, index) => ({ value: String(index + 1), label }));
}

export function yearOptions(years: readonly number[]): { value: string; label: string }[] {
  return years.map((year) => ({ value: String(year), label: String(year) }));
}
