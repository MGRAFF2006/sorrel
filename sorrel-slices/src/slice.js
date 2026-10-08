import fs from "node:fs";
import path from "node:path";

const SUPPORTED_EXTENSIONS = [".ts", ".tsx", ".js", ".jsx", ".json"];
const DEFAULT_INCLUDE_PATTERNS = ["**/*"];

export class SliceError extends Error {
  constructor(message) {
    super(message);
    this.name = "SliceError";
  }
}

export function createSliceManifest(options) {
  const projectRoot = resolveProjectRoot(options?.projectRoot);
  const entrypoints = normalizeEntrypoints(options?.entrypoints ?? options?.entrypoint, projectRoot);
  const includePatterns = normalizePatterns(options?.includePatterns, DEFAULT_INCLUDE_PATTERNS);
  const excludePatterns = normalizePatterns(options?.excludePatterns, []);
  const includeMatchers = includePatterns.map(compileGlob);
  const excludeMatchers = excludePatterns.map(compileGlob);

  const included = new Set();
  const queued = [];
  const excluded = new Map();
  const unresolved = new Map();

  for (const entrypoint of entrypoints) {
    const rel = toProjectPath(projectRoot, path.resolve(projectRoot, entrypoint));
    const abs = path.join(projectRoot, rel);

    if (!fileExists(abs)) {
      throw new SliceError(`Entrypoint does not exist: ${rel}`);
    }

    const inclusion = getInclusion(rel, includeMatchers, excludeMatchers);
    if (!inclusion.included) {
      throw new SliceError(`Entrypoint is excluded by slice patterns: ${rel}`);
    }

    queueFile(rel, included, queued);
  }

  for (let index = 0; index < queued.length; index += 1) {
    const fromRel = queued[index];
    const fromAbs = path.join(projectRoot, fromRel);
    const imports = parseImports(fs.readFileSync(fromAbs, "utf8"));

    for (const importRef of imports) {
      if (importRef.kind === "dynamic") {
        addUnresolved(unresolved, {
          from: fromRel,
          specifier: importRef.specifier,
          reason: "dynamic_import"
        });
        continue;
      }

      const resolution = resolveImport(projectRoot, fromAbs, importRef.specifier);
      if (!resolution.resolved) {
        addUnresolved(unresolved, {
          from: fromRel,
          specifier: importRef.specifier,
          reason: resolution.reason
        });
        continue;
      }

      const inclusion = getInclusion(resolution.path, includeMatchers, excludeMatchers);
      if (!inclusion.included) {
        addExcluded(excluded, {
          path: resolution.path,
          reason: inclusion.reason,
          pattern: inclusion.pattern
        });
        continue;
      }

      queueFile(resolution.path, included, queued);
    }
  }

  const packageMetadata = detectPackageMetadata(projectRoot, included, includeMatchers, excludeMatchers, excluded);
  for (const metadata of packageMetadata) {
    queueFile(metadata.path, included, queued);
  }

  return {
    schemaVersion: "sorrel.slices.manifest.v0",
    kind: "SliceManifest",
    language: "typescript-javascript",
    sourceRoot: ".",
    entrypoints: sortStrings(entrypoints),
    includePatterns,
    excludePatterns,
    includedFiles: sortStrings([...included]),
    excludedFiles: sortObjects([...excluded.values()], ["path", "reason", "pattern"]),
    unresolvedImports: sortObjects([...unresolved.values()], ["from", "specifier", "reason"]),
    detectedPackageMetadata: packageMetadata,
    suggestedTargetRepoName: suggestTargetRepoName(projectRoot, entrypoints, packageMetadata)
  };
}

export function parseImports(source) {
  const { masked, codePositions } = maskCommentsAndStrings(source);
  const imports = [];
  const patterns = [
    { kind: "static", syntax: "import", regex: /\bimport\s+(?:type\s+)?(?:[^;"']*?\s+from\s*)?("(?:\\(?:\r\n|[\s\S])|[^"\\\r\n])*"|'(?:\\(?:\r\n|[\s\S])|[^'\\\r\n])*')/g },
    { kind: "static", syntax: "export", regex: /\bexport\s+(?:type\s+)?[^;"']*?\bfrom\s*("(?:\\(?:\r\n|[\s\S])|[^"\\\r\n])*"|'(?:\\(?:\r\n|[\s\S])|[^'\\\r\n])*')/g },
    { kind: "static", syntax: "require", regex: /\brequire\s*\(\s*("(?:\\(?:\r\n|[\s\S])|[^"\\\r\n])*"|'(?:\\(?:\r\n|[\s\S])|[^'\\\r\n])*')\s*\)/g },
    { kind: "dynamic", syntax: "import", regex: /\bimport\s*\(\s*("(?:\\(?:\r\n|[\s\S])|[^"\\\r\n])*"|'(?:\\(?:\r\n|[\s\S])|[^'\\\r\n])*')\s*\)/g }
  ];

  for (const pattern of patterns) {
    let match;
    while ((match = pattern.regex.exec(masked)) !== null) {
      if (!codePositions.has(match.index)) {
        pattern.regex.lastIndex = match.index + 1;
        continue;
      }
      const specifier = decodeModuleString(match[1]);
      if (specifier === undefined) continue;
      imports.push({
        kind: pattern.kind,
        syntax: pattern.syntax,
        specifier,
        index: match.index
      });
    }
  }

  return sortObjects(dedupeImports(imports), ["index", "kind", "syntax", "specifier"]).map(({ index: _index, ...item }) => item);
}

// Decode literal spelling, never evaluate source or resolve bindings.
function decodeModuleString(literal) {
  const source = literal.slice(1, -1);
  const controls = { b: "\b", f: "\f", n: "\n", r: "\r", t: "\t", v: "\v" };
  let value = "";
  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    if (char !== "\\") {
      value += char;
      continue;
    }
    const escape = source[++index];
    if (escape === undefined) return undefined;
    if (escape === "\n" || escape === "\u2028" || escape === "\u2029") continue;
    if (escape === "\r") {
      if (source[index + 1] === "\n") index += 1;
      continue;
    }
    if (escape === "x" || escape === "u") {
      const tail = source.slice(index + 1);
      const braced = escape === "u" && tail.startsWith("{");
      const match = tail.match(braced ? /^\{([0-9a-f]+)\}/i : escape === "x" ? /^([0-9a-f]{2})/i : /^([0-9a-f]{4})/i);
      if (!match) return undefined;
      const point = Number.parseInt(match[1], 16);
      if (point > 0x10ffff) return undefined;
      value += String.fromCodePoint(point);
      index += match[0].length;
    } else if (/[0-7]/.test(escape)) {
      // Legacy CommonJS strings may contain octal escapes.
      const digits = source.slice(index).match(escape <= "3" ? /^[0-7]{1,3}/ : /^[0-7]{1,2}/)[0];
      value += String.fromCharCode(Number.parseInt(digits, 8));
      index += digits.length - 1;
    } else {
      value += controls[escape] ?? escape;
    }
  }
  return value;
}

function resolveProjectRoot(projectRoot) {
  if (!projectRoot) {
    throw new SliceError("projectRoot is required");
  }

  const resolved = path.resolve(String(projectRoot));
  if (!directoryExists(resolved)) {
    throw new SliceError(`Project root does not exist or is not a directory: ${projectRoot}`);
  }

  return resolved;
}

function normalizeEntrypoints(entrypointInput, projectRoot) {
  const rawEntrypoints = Array.isArray(entrypointInput) ? entrypointInput : [entrypointInput];
  const entrypoints = rawEntrypoints
    .filter((entrypoint) => entrypoint !== undefined && entrypoint !== null && String(entrypoint).length > 0)
    .map((entrypoint) => toProjectPath(projectRoot, path.resolve(projectRoot, String(entrypoint))));

  if (entrypoints.length === 0) {
    throw new SliceError("At least one entrypoint is required");
  }

  return sortStrings([...new Set(entrypoints)]);
}

function normalizePatterns(patterns, defaults) {
  if (!patterns || patterns.length === 0) {
    return [...defaults];
  }

  const rawPatterns = Array.isArray(patterns) ? patterns : [patterns];
  return sortStrings(
    rawPatterns
      .map((pattern) => normalizeProjectPath(String(pattern)))
      .filter((pattern) => pattern.length > 0)
  );
}

function compileGlob(pattern) {
  const normalized = normalizeProjectPath(pattern.endsWith("/") ? `${pattern}**` : pattern);
  let regex = "^";

  for (let index = 0; index < normalized.length; index += 1) {
    const char = normalized[index];
    const next = normalized[index + 1];

    if (char === "*" && next === "*") {
      const after = normalized[index + 2];
      if (after === "/") {
        regex += "(?:.*/)?";
        index += 2;
      } else {
        regex += ".*";
        index += 1;
      }
      continue;
    }

    if (char === "*") {
      regex += "[^/]*";
      continue;
    }

    if (char === "?") {
      regex += "[^/]";
      continue;
    }

    regex += escapeRegex(char);
  }

  regex += "$";
  return { pattern: normalized, regex: new RegExp(regex) };
}

function getInclusion(relPath, includeMatchers, excludeMatchers) {
  const includeMatch = firstMatch(relPath, includeMatchers);
  if (!includeMatch) {
    return { included: false, reason: "not_included" };
  }

  const excludeMatch = firstMatch(relPath, excludeMatchers);
  if (excludeMatch) {
    return { included: false, reason: "exclude_pattern", pattern: excludeMatch.pattern };
  }

  return { included: true };
}

function firstMatch(relPath, matchers) {
  return matchers.find((matcher) => matcher.regex.test(relPath));
}

function queueFile(relPath, included, queued) {
  if (!included.has(relPath)) {
    included.add(relPath);
    queued.push(relPath);
  }
}

function addExcluded(excluded, item) {
  const key = `${item.path}\0${item.reason}\0${item.pattern ?? ""}`;
  excluded.set(key, item);
}

function addUnresolved(unresolved, item) {
  const key = `${item.from}\0${item.specifier}\0${item.reason}`;
  unresolved.set(key, item);
}

function resolveImport(projectRoot, fromAbs, specifier) {
  if (!isLocalSpecifier(specifier)) {
    return { resolved: false, reason: "external_package" };
  }

  const targetBase = path.resolve(path.dirname(fromAbs), specifier);
  if (!isInside(projectRoot, targetBase)) {
    return { resolved: false, reason: "outside_project_root" };
  }

  const ext = path.extname(targetBase);
  if (ext && !SUPPORTED_EXTENSIONS.includes(ext)) {
    return { resolved: false, reason: "unsupported_extension" };
  }

  const candidates = [];
  if (ext) {
    candidates.push(targetBase);
  } else {
    for (const supportedExt of SUPPORTED_EXTENSIONS) {
      candidates.push(`${targetBase}${supportedExt}`);
    }
  }

  if (directoryExists(targetBase)) {
    for (const supportedExt of SUPPORTED_EXTENSIONS) {
      candidates.push(path.join(targetBase, `index${supportedExt}`));
    }
  }

  const resolved = candidates.find(fileExists);
  if (!resolved) {
    return { resolved: false, reason: "not_found" };
  }

  return { resolved: true, path: toProjectPath(projectRoot, resolved) };
}

function detectPackageMetadata(projectRoot, included, includeMatchers, excludeMatchers, excluded) {
  const metadataPaths = new Set();
  for (const relPath of included) {
    let currentDir = path.dirname(path.join(projectRoot, relPath));

    while (isInside(projectRoot, currentDir)) {
      for (const fileName of ["package.json", "tsconfig.json"]) {
        const candidateAbs = path.join(currentDir, fileName);
        if (fileExists(candidateAbs)) {
          const candidateRel = toProjectPath(projectRoot, candidateAbs);
          const inclusion = getInclusion(candidateRel, includeMatchers, excludeMatchers);
          if (inclusion.included) {
            metadataPaths.add(candidateRel);
          } else {
            addExcluded(excluded, {
              path: candidateRel,
              reason: inclusion.reason,
              pattern: inclusion.pattern
            });
          }
        }
      }

      if (samePath(currentDir, projectRoot)) {
        break;
      }

      currentDir = path.dirname(currentDir);
    }
  }

  return sortStrings([...metadataPaths]).map((relPath) => readMetadata(projectRoot, relPath));
}

function readMetadata(projectRoot, relPath) {
  if (path.basename(relPath) === "package.json") {
    const raw = fs.readFileSync(path.join(projectRoot, relPath), "utf8");
    const packageJson = JSON.parse(raw);
    return {
      type: "package.json",
      path: relPath,
      name: packageJson.name,
      version: packageJson.version,
      private: packageJson.private
    };
  }

  return {
    type: "tsconfig.json",
    path: relPath
  };
}

function suggestTargetRepoName(projectRoot, entrypoints, packageMetadata) {
  const packageName = nearestEntrypointPackageName(entrypoints[0], packageMetadata);
  const fallback = `${path.basename(projectRoot)}-${path.basename(entrypoints[0], path.extname(entrypoints[0]))}`;
  return sanitizeRepoName(packageName ?? fallback);
}

function nearestEntrypointPackageName(entrypoint, packageMetadata) {
  const packages = packageMetadata
    .filter((metadata) => metadata.type === "package.json" && metadata.name)
    .map((metadata) => ({
      ...metadata,
      directory: path.dirname(metadata.path) === "." ? "" : path.dirname(metadata.path)
    }))
    .filter((metadata) => metadata.directory === "" || entrypoint.startsWith(`${metadata.directory}/`))
    .sort((left, right) => right.directory.length - left.directory.length);

  return packages[0]?.name;
}

function sanitizeRepoName(name) {
  const sanitized = String(name)
    .replace(/^@/, "")
    .replace(/\//g, "-")
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .replace(/-{2,}/g, "-");

  return sanitized || "sorrel-slice";
}

function dedupeImports(imports) {
  const seen = new Set();
  const deduped = [];
  for (const importRef of imports) {
    const key = `${importRef.kind}\0${importRef.syntax}\0${importRef.specifier}\0${importRef.index}`;
    if (!seen.has(key)) {
      seen.add(key);
      deduped.push(importRef);
    }
  }

  return deduped;
}

function maskCommentsAndStrings(source) {
  let result = "";
  let state = "code";
  let escaped = false;
  let regexAllowed = true;
  let regexCharacterClass = false;
  let lastToken = "";
  const controlParentheses = [];
  const identifiers = /[$_\p{ID_Start}][$\u200C\u200D\p{ID_Continue}]*/uy;
  const codePositions = new Set();
  const templateExpressions = [];

  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    const next = source[index + 1];

    if (state === "code") {
      identifiers.lastIndex = index;
      const identifier = identifiers.exec(source);
      if (char === "/" && next === "/") {
        result += "  ";
        state = "lineComment";
        index += 1;
      } else if (char === "/" && next === "*") {
        result += "  ";
        state = "blockComment";
        index += 1;
      } else if (char === "/" && regexAllowed) {
        result += " ";
        state = "regex";
        escaped = false;
        regexCharacterClass = false;
      } else if (identifier) {
        const word = identifier[0];
        const end = index + word.length;
        const property = lastToken === ".";
        lastToken = property ? `.${word}` : word;
        if (!property) codePositions.add(index);
        result += word;
        regexAllowed = /^(return|throw|case|delete|void|typeof|instanceof|in|new|else|do)$/.test(lastToken);
        index = end - 1;
      } else if ((char === "+" || char === "-") && next === char) {
        codePositions.add(index);
        codePositions.add(index + 1);
        result += char + next;
        index += 1;
        lastToken = char + next;
      } else {
        codePositions.add(index);
        result += char;
        if (templateExpressions.length > 0) {
          if (char === "{") templateExpressions[templateExpressions.length - 1] += 1;
          if (char === "}" && --templateExpressions[templateExpressions.length - 1] === 0) {
            templateExpressions.pop();
            state = "template";
          }
        }
        if (char === "'") {
          state = "singleQuote";
          escaped = false;
        } else if (char === "\"") {
          state = "doubleQuote";
          escaped = false;
        } else if (char === "`") {
          state = "template";
          escaped = false;
        }
        if (!/\s/.test(char)) {
          if (char === "(") {
            controlParentheses.push(/^(if|while|for|with|switch|catch)$/.test(lastToken));
            regexAllowed = true;
          } else if (char === ")") {
            regexAllowed = controlParentheses.pop() ?? false;
          } else {
            regexAllowed = !/[\w$\].}'"`]/.test(char);
          }
          lastToken = char;
        }
      }
      continue;
    }

    if (state === "lineComment") {
      if (char === "\n") {
        result += "\n";
        state = "code";
      } else {
        result += " ";
      }
      continue;
    }

    if (state === "blockComment") {
      if (char === "*" && next === "/") {
        result += "  ";
        state = "code";
        index += 1;
      } else {
        result += char === "\n" ? "\n" : " ";
      }
      continue;
    }

    if (state === "regex") {
      result += char === "\n" ? "\n" : " ";
      if (escaped) {
        escaped = false;
      } else if (char === "\\") {
        escaped = true;
      } else if (char === "[") {
        regexCharacterClass = true;
      } else if (char === "]") {
        regexCharacterClass = false;
      } else if (char === "/" && !regexCharacterClass) {
        state = "code";
        regexAllowed = false;
        lastToken = "/";
      }
      continue;
    }

    result += char;

    if (escaped) {
      escaped = false;
      continue;
    }

    if (char === "\\") {
      escaped = true;
      continue;
    }

    if (state === "template" && char === "$" && next === "{") {
      result += next;
      index += 1;
      templateExpressions.push(1);
      state = "code";
      regexAllowed = true;
      lastToken = "{";
      continue;
    }

    if (
      (state === "singleQuote" && char === "'") ||
      (state === "doubleQuote" && char === "\"") ||
      (state === "template" && char === "`")
    ) {
      state = "code";
      regexAllowed = false;
      lastToken = char;
    }
  }

  return { masked: result, codePositions };
}

function isLocalSpecifier(specifier) {
  return specifier === "." || specifier === ".." || specifier.startsWith("./") || specifier.startsWith("../");
}

function toProjectPath(projectRoot, absPath) {
  if (!isInside(projectRoot, absPath)) {
    throw new SliceError(`Path is outside project root: ${absPath}`);
  }

  return normalizeProjectPath(path.relative(projectRoot, absPath));
}

function normalizeProjectPath(value) {
  const normalized = value.replaceAll("\\", "/").replace(/^\.\//, "");
  return normalized === "" ? "." : normalized;
}

function isInside(parent, candidate) {
  const relative = path.relative(parent, candidate);
  return relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative));
}

function samePath(left, right) {
  return path.resolve(left) === path.resolve(right);
}

function fileExists(filePath) {
  try {
    return fs.statSync(filePath).isFile();
  } catch {
    return false;
  }
}

function directoryExists(directoryPath) {
  try {
    return fs.statSync(directoryPath).isDirectory();
  } catch {
    return false;
  }
}

function escapeRegex(char) {
  return char.replace(/[|\\{}()[\]^$+?.]/g, "\\$&");
}

function sortStrings(values) {
  return values.sort(compareValues);
}

function sortObjects(values, keys) {
  return values.sort((left, right) => {
    for (const key of keys) {
      const comparison = compareValues(left[key] ?? "", right[key] ?? "");
      if (comparison !== 0) {
        return comparison;
      }
    }

    return 0;
  });
}

function compareValues(left, right) {
  if (typeof left === "number" && typeof right === "number") {
    return left - right;
  }

  const leftString = String(left);
  const rightString = String(right);
  if (leftString < rightString) {
    return -1;
  }

  if (leftString > rightString) {
    return 1;
  }

  return 0;
}
