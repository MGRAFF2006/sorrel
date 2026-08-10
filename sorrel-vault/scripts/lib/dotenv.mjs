import { readFile } from "node:fs/promises";

const KEY_PATTERN = /^[A-Za-z_][A-Za-z0-9_]*$/;

export function parseDotEnv(source, { filePath = "<inline>" } = {}) {
  const values = new Map();
  const lines = source.split(/\r?\n/);

  for (const [index, rawLine] of lines.entries()) {
    const lineNumber = index + 1;
    const line = rawLine.trim();

    if (line === "" || line.startsWith("#")) {
      continue;
    }

    const withoutExport = line.startsWith("export ") ? line.slice("export ".length).trimStart() : line;
    const separatorIndex = withoutExport.indexOf("=");

    if (separatorIndex === -1) {
      throw new Error(`${filePath}:${lineNumber}: expected KEY=VALUE`);
    }

    const key = withoutExport.slice(0, separatorIndex).trim();
    const rawValue = withoutExport.slice(separatorIndex + 1).trim();

    if (!KEY_PATTERN.test(key)) {
      throw new Error(`${filePath}:${lineNumber}: invalid environment key ${JSON.stringify(key)}`);
    }

    values.set(key, parseValue(rawValue));
  }

  return values;
}

export async function loadDotEnvFile(filePath) {
  const source = await readFile(filePath, "utf8");
  return parseDotEnv(source, { filePath });
}

function parseValue(rawValue) {
  if (rawValue.length < 2) {
    return rawValue;
  }

  const quote = rawValue[0];
  const last = rawValue.at(-1);

  if ((quote === `"` || quote === "'") && last === quote) {
    const inner = rawValue.slice(1, -1);
    return quote === `"` ? unescapeDoubleQuoted(inner) : inner;
  }

  return stripInlineComment(rawValue);
}

function stripInlineComment(value) {
  const hashIndex = value.search(/\s#/);
  return hashIndex === -1 ? value : value.slice(0, hashIndex).trimEnd();
}

function unescapeDoubleQuoted(value) {
  return value.replace(/\\([nrt"\\])/g, (_match, escaped) => {
    switch (escaped) {
      case "n":
        return "\n";
      case "r":
        return "\r";
      case "t":
        return "\t";
      case `"`:
        return `"`;
      case "\\":
        return "\\";
      default:
        return escaped;
    }
  });
}
