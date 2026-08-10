#!/usr/bin/env node

import fs from "node:fs";
import { createSliceManifest, SliceError } from "./slice.js";

function main(argv) {
  const options = parseArgs(argv);

  if (options.help) {
    process.stdout.write(`${usage()}\n`);
    return;
  }

  const manifest = createSliceManifest({
    projectRoot: options.projectRoot,
    entrypoints: options.entrypoints,
    includePatterns: options.includePatterns,
    excludePatterns: options.excludePatterns
  });
  const json = `${JSON.stringify(manifest, null, 2)}\n`;

  if (options.out) {
    fs.writeFileSync(options.out, json);
  } else {
    process.stdout.write(json);
  }
}

function parseArgs(argv) {
  const options = {
    entrypoints: [],
    includePatterns: [],
    excludePatterns: []
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];

    if (arg === "--help" || arg === "-h") {
      options.help = true;
      continue;
    }

    if (arg === "--project-root" || arg === "-r") {
      options.projectRoot = readValue(argv, ++index, arg);
      continue;
    }

    if (arg === "--entrypoint" || arg === "-e") {
      options.entrypoints.push(readValue(argv, ++index, arg));
      continue;
    }

    if (arg === "--include" || arg === "-i") {
      options.includePatterns.push(readValue(argv, ++index, arg));
      continue;
    }

    if (arg === "--exclude" || arg === "-x") {
      options.excludePatterns.push(readValue(argv, ++index, arg));
      continue;
    }

    if (arg === "--out" || arg === "-o") {
      options.out = readValue(argv, ++index, arg);
      continue;
    }

    throw new SliceError(`Unknown option: ${arg}`);
  }

  return options;
}

function readValue(argv, index, optionName) {
  const value = argv[index];
  if (!value || value.startsWith("-")) {
    throw new SliceError(`${optionName} requires a value`);
  }

  return value;
}

function usage() {
  return `Usage:
  sorrel-slices --project-root <path> --entrypoint <path> [options]

Options:
  -r, --project-root <path>  Project root that slice paths are relative to.
  -e, --entrypoint <path>    Entry file relative to the project root. Repeatable.
  -i, --include <glob>       Include glob relative to the project root. Repeatable.
  -x, --exclude <glob>       Exclude glob relative to the project root. Repeatable.
  -o, --out <path>           Write the manifest to a file instead of stdout.
  -h, --help                 Show this help text.`;
}

try {
  main(process.argv.slice(2));
} catch (error) {
  if (error instanceof SliceError) {
    process.stderr.write(`sorrel-slices: ${error.message}\n`);
    process.exitCode = 1;
  } else {
    throw error;
  }
}
