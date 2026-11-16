// A failure is shown as a short summary written by the call site. The technical
// text from the backend stays separate, in a collapsed and copyable section.
export function technicalDetail(summary: string, detail: string | null | undefined): string | null {
  const text = detail?.trim() ?? "";
  return text === "" || text === summary.trim() ? null : text;
}

export function failureReport(summary: string, detail: string | null | undefined): string {
  const technical = technicalDetail(summary, detail);
  return technical === null ? summary : `${summary}\n\n${technical}`;
}
