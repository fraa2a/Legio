export function parseLaunchArguments(value: string): string[] {
  const args: string[] = [];
  let current = "";
  let quote: "'" | '"' | null = null;
  let escaped = false;
  let active = false;

  for (const character of value) {
    if (escaped) {
      current += character;
      escaped = false;
      active = true;
    } else if (character === "\\" && quote === '"') {
      escaped = true;
    } else if (quote !== null) {
      if (character === quote) quote = null;
      else current += character;
    } else if (character === "'" || character === '"') {
      quote = character;
      active = true;
    } else if (/\s/.test(character)) {
      if (active) args.push(current);
      current = "";
      active = false;
    } else {
      current += character;
      active = true;
    }
  }

  if (quote !== null) throw new Error("Chiudi le virgolette negli argomenti di avvio.");
  if (escaped) current += "\\";
  if (active || escaped) args.push(current);
  return args;
}

export function formatLaunchArguments(args: string[]): string {
  return args
    .map((argument) => {
      if (argument === "") return '""';
      if (!/[\s"'\\]/.test(argument)) return argument;
      return `"${argument.replace(/["\\]/g, "\\$&")}"`;
    })
    .join(" ");
}
