export function parseValues(input: string): string[] {
  const values: string[] = [];
  let value = '',
    quoted = false,
    afterQuote = false;
  for (let i = 0; i < input.length; i++) {
    const ch = input[i]!;
    if (quoted) {
      if (ch === "'") {
        if (input[i + 1] === "'") {
          value += "'";
          i++;
        } else {
          quoted = false;
          afterQuote = true;
        }
      } else value += ch;
    } else if (ch === ',' || ch === '\n') {
      values.push(afterQuote ? value : value.trim());
      value = '';
      afterQuote = false;
    } else if (ch === "'" && !value.trim() && !afterQuote) {
      quoted = true;
      value = '';
    } else if (afterQuote && !/\s/.test(ch))
      throw new Error('Separate quoted ENUM/SET values with commas.');
    else if (!afterQuote) value += ch;
  }
  if (quoted) throw new Error('ENUM/SET values contain an unclosed quote.');
  values.push(afterQuote ? value : value.trim());
  if (!input.trim() || values.some((v) => !v.length))
    throw new Error('Enter non-empty ENUM/SET values separated by commas.');
  if (new Set(values).size !== values.length) throw new Error('ENUM/SET values must be unique.');
  return values;
}
