export const pageSize = 20;

export function pageCountFor(itemCount: number): number {
  return Math.max(1, Math.ceil(itemCount / pageSize));
}

export function pageItems<T>(items: readonly T[], page: number): T[] {
  return items.slice((page - 1) * pageSize, page * pageSize);
}

// First, last and the neighbours of the current page; "gap" stands for skipped pages.
export function pageNumbers(page: number, pageCount: number): (number | "gap")[] {
  const wanted = [...new Set([1, page - 1, page, page + 1, pageCount])]
    .filter((number) => number >= 1 && number <= pageCount)
    .sort((left, right) => left - right);
  const result: (number | "gap")[] = [];
  for (const [index, number] of wanted.entries()) {
    const previous = wanted[index - 1];
    if (previous !== undefined && number - previous === 2) result.push(previous + 1);
    else if (previous !== undefined && number - previous > 2) result.push("gap");
    result.push(number);
  }
  return result;
}
