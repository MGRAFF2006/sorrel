#!/usr/bin/env node

import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const CATEGORIES = ['Added', 'Changed', 'Deprecated', 'Removed', 'Fixed', 'Security'];

export function categorizeChange(change) {
  const labels = new Set(change.labels.map((label) => label.toLowerCase()));
  if (labels.has('security')) return 'Security';
  if (labels.has('breaking-change') || labels.has('breaking')) return 'Changed';
  if (labels.has('bug') || labels.has('fix')) return 'Fixed';
  if (labels.has('enhancement') || labels.has('feature')) return 'Added';

  const conventional = change.title.match(/^([a-z]+)(?:\([^)]*\))?(!)?:\s*(.+)$/i);
  const type = conventional?.[1].toLowerCase();
  if (conventional?.[2]) return 'Changed';
  if (type === 'feat') return 'Added';
  if (type === 'fix') return 'Fixed';
  if (type === 'security') return 'Security';
  if (type === 'deprecate' || type === 'deprecated') return 'Deprecated';
  if (type === 'remove' || type === 'removed') return 'Removed';
  if (/^(add|create|enable|implement|introduce|ship|support)\b/i.test(change.title)) {
    return 'Added';
  }
  if (/^(correct|fix|guard|prevent|repair|reject)\b/i.test(change.title)) {
    return 'Fixed';
  }
  if (/^(delete|drop|eliminate|remove)\b/i.test(change.title)) return 'Removed';
  if (/^(harden|secure)\b/i.test(change.title)) return 'Security';
  return 'Changed';
}

export function displayTitle(title) {
  const conventional = title.match(/^[a-z]+(?:\([^)]*\))?!?:\s*(.+)$/i);
  const summary = (conventional?.[1] ?? title).trim().replace(/[.!]+$/, '');
  return summary.length === 0
    ? 'Updated Sorrel'
    : `${summary[0].toUpperCase()}${summary.slice(1)}`;
}

export function renderRelease(changes) {
  if (changes.length === 0) return 'No package-specific changes.';

  const grouped = new Map(CATEGORIES.map((category) => [category, []]));
  for (const change of changes) grouped.get(categorizeChange(change)).push(change);

  return CATEGORIES.flatMap((category) => {
    const entries = grouped.get(category);
    if (entries.length === 0) return [];
    const lines = entries.map((change) => {
      const reference = change.number
        ? ` ([#${change.number}](${change.url}))`
        : '';
      return `- ${displayTitle(change.title)}${reference}.`;
    });
    return [`### ${category}`, '', ...lines, ''];
  }).join('\n').trimEnd();
}

export function updateChangelog(text, { version, date, changes, repository }) {
  if (text.includes(`## [${version}]`)) {
    throw new Error(`changelog already contains release ${version}`);
  }

  const unreleased = /^## \[Unreleased\][^\n]*\n/m.exec(text);
  if (!unreleased) throw new Error('changelog has no [Unreleased] section');
  const bodyStart = unreleased.index + unreleased[0].length;
  const nextHeadingOffset = text.slice(bodyStart).search(/^## /m);
  if (nextHeadingOffset < 0) throw new Error('changelog has no prior release section');
  const nextHeading = bodyStart + nextHeadingOffset;
  const release = renderRelease(changes);
  let updated = `${text.slice(0, bodyStart)}\nNo changes yet.\n\n## [${version}] - ${date}\n\n${release}\n\n${text.slice(nextHeading)}`;

  updated = updated.replace(/^\[Unreleased\]:.*\n?/m, '');
  const versionDefinition = new RegExp(`^\\[${escapeRegExp(version)}\\]:.*\\n?`, 'm');
  updated = updated.replace(versionDefinition, '');
  return `${updated.trimEnd()}\n\n[Unreleased]: https://github.com/${repository}/compare/v${version}...HEAD\n[${version}]: https://github.com/${repository}/releases/tag/v${version}\n`;
}

export function packagesForChange(change, modules) {
  return modules.filter((module) =>
    change.files.some((path) => path === module || path.startsWith(`${module}/`)),
  );
}

export function generateChangelogs({ changes, version, date, repository, root = ROOT }) {
  const manifest = JSON.parse(readFileSync(join(root, 'release/manifest.json'), 'utf8'));
  const modules = Object.keys(manifest.modules).filter((module) =>
    existsSync(join(root, module, 'CHANGELOG.md')),
  );
  const included = changes
    .filter((change) => !change.labels.some((label) => label.toLowerCase() === 'skip-changelog'))
    .sort((left, right) => (left.number ?? 0) - (right.number ?? 0));

  const paths = ['CHANGELOG.md', ...modules.map((module) => `${module}/CHANGELOG.md`)];
  for (const path of paths) {
    const module = path === 'CHANGELOG.md' ? null : path.split('/')[0];
    const selected = module
      ? included.filter((change) => packagesForChange(change, [module]).length > 0)
      : included;
    const absolute = join(root, path);
    const updated = updateChangelog(readFileSync(absolute, 'utf8'), {
      version,
      date,
      changes: selected,
      repository,
    });
    writeFileSync(absolute, updated);
  }

  return { changes: included.length, modules: modules.length };
}

async function fetchChanges({ repository, base, head, token }) {
  const commits = [];
  for (let page = 1; ; page += 1) {
    const comparison = await github(
      repository,
      `/compare/${encodeURIComponent(base)}...${encodeURIComponent(head)}?per_page=100&page=${page}`,
      token,
    );
    commits.push(...comparison.commits);
    if (comparison.commits.length < 100) break;
  }

  const pulls = new Map();
  const direct = [];
  for (const commit of commits) {
    const associated = await github(
      repository,
      `/commits/${commit.sha}/pulls?per_page=100`,
      token,
    );
    const merged = associated.filter((pull) => pull.merged_at);
    if (merged.length === 0) {
      direct.push({
        number: null,
        title: commit.commit.message.split('\n')[0],
        url: commit.html_url,
        labels: [],
        files: [],
      });
    }
    for (const pull of merged) pulls.set(pull.number, pull);
  }

  const changes = [];
  for (const pull of pulls.values()) {
    const files = [];
    for (let page = 1; ; page += 1) {
      const batch = await github(
        repository,
        `/pulls/${pull.number}/files?per_page=100&page=${page}`,
        token,
      );
      files.push(...batch.map((file) => file.filename));
      if (batch.length < 100) break;
    }
    changes.push({
      number: pull.number,
      title: pull.title,
      url: pull.html_url,
      labels: pull.labels.map((label) => label.name),
      files,
    });
  }
  return [...changes, ...direct];
}

async function github(repository, path, token) {
  const response = await fetch(`https://api.github.com/repos/${repository}${path}`, {
    headers: {
      Accept: 'application/vnd.github+json',
      Authorization: `Bearer ${token}`,
      'X-GitHub-Api-Version': '2022-11-28',
      'User-Agent': 'sorrel-changelog-preparer',
    },
  });
  if (!response.ok) {
    throw new Error(`GitHub API ${response.status} for ${path}: ${await response.text()}`);
  }
  return response.json();
}

function parseArguments(argv) {
  const values = new Map();
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index];
    const value = argv[index + 1];
    if (!key?.startsWith('--') || value === undefined) {
      throw new Error('usage: prepare-changelogs --version VERSION --date YYYY-MM-DD [--base TAG] [--head REF] [--input FILE]');
    }
    values.set(key.slice(2), value);
  }
  return values;
}

function validateVersion(version) {
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error(`invalid semantic version: ${version}`);
  }
}

function validateDate(date) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date) || Number.isNaN(Date.parse(`${date}T00:00:00Z`))) {
    throw new Error(`invalid release date: ${date}`);
  }
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

async function main() {
  const args = parseArguments(process.argv.slice(2));
  const version = args.get('version');
  const date = args.get('date');
  if (!version || !date) throw new Error('--version and --date are required');
  validateVersion(version);
  validateDate(date);

  const manifest = JSON.parse(readFileSync(join(ROOT, 'release/manifest.json'), 'utf8'));
  const repository = process.env.GITHUB_REPOSITORY ?? 'MGRAFF2006/sorrel';
  const input = args.get('input');
  const token = process.env.GH_TOKEN ?? process.env.GITHUB_TOKEN;
  if (!input && !token) {
    throw new Error('GH_TOKEN or GITHUB_TOKEN is required when --input is omitted');
  }
  const changes = input
    ? JSON.parse(readFileSync(resolve(input), 'utf8'))
    : await fetchChanges({
        repository,
        base: args.get('base') ?? manifest.release,
        head: args.get('head') ?? 'main',
        token,
      });

  const result = generateChangelogs({ changes, version, date, repository });
  console.log(`Generated ${version} changelogs from ${result.changes} change(s) across ${result.modules} package changelog(s).`);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
