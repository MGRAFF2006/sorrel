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
  const masked = maskCommentsAndStrings(source);
  const imports = [];
  const patterns = [
    { kind: "static", syntax: "import", regex: /\bimport\s+(?:type\s+)?(?:[^;"']*?\s+from\s*)?["']([^"']+)["']/g },
    { kind: "static", syntax: "export", regex: /\bexport\s+(?:type\s+)?[^;"']*?\bfrom\s*["']([^"']+)["']/g },
    { kind: "static", syntax: "require", regex: /\brequire\s*\(\s*["']([^"']+)["']\s*\)/g },
    { kind: "dynamic", syntax: "import", regex: /\bimport\s*\(\s*["']([^"']+)["']\s*\)/g }
  ];

  for (const pattern of patterns) {
    let match;
    while ((match = pattern.regex.exec(masked)) !== null) {
      imports.push({
        kind: pattern.kind,
        syntax: pattern.syntax,
        specifier: match[1],
        index: match.index
      });
    }
  }

  return sortObjects(dedupeImports(imports), ["index", "kind", "syntax", "specifier"]).map(({ index: _index, ...item }) => item);
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

  for (let index = 0; index < source.length; index += 1) {
    const char = source[index];
    const next = source[index + 1];

    if (state === "code") {
      if (char === "/" && next === "/") {
        result += "  ";
        state = "lineComment";
        index += 1;
      } else if (char === "/" && next === "*") {
        result += "  ";
        state = "blockComment";
        index += 1;
      } else {
        result += char;
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

    result += char;

    if (escaped) {
      escaped = false;
      continue;
    }

    if (char === "\\") {
      escaped = true;
      continue;
    }

    if (
      (state === "singleQuote" && char === "'") ||
      (state === "doubleQuote" && char === "\"") ||
      (state === "template" && char === "`")
    ) {
      state = "code";
    }
  }

  return result;
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
