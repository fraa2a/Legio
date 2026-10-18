import { t } from "../i18n";
export function parseLaunchArguments(value: string): string[] {
  const args: string[] = [];
  let current = "";
  let quote: "'" | '"' | null = null;
  let active = false;

  for (let index = 0; index < value.length; index++) {
    const character = value[index];
    if (character === "\\" && quote === '"') {
      const start = index;
      while (value[index + 1] === "\\") index++;
      const count = index - start + 1;
      if (value[index + 1] === '"') {
        current += "\\".repeat(Math.floor(count / 2));
        index++;
        if (count % 2 === 1) current += '"';
        else quote = null;
      } else {
        current += "\\".repeat(count);
      }
      active = true;
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

  if (quote !== null) throw new Error(t("Chiudi le virgolette negli argomenti di avvio.", undefined));
  if (active) args.push(current);
  return args;
}

export function formatLaunchArguments(args: string[]): string {
  return args
    .map((argument) => {
      if (argument === "") return '""';
      if (!/[\s"'\\]/.test(argument)) return argument;
      return `"${argument.replace(/(\\*)"/g, '$1$1\\"').replace(/(\\+)$/g, "$1$1")}"`;
    })
    .join(" ");
}
