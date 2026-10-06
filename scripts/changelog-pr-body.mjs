#!/usr/bin/env node

import { readFileSync } from 'node:fs';
import { renderChangelogPullRequestBody } from './prepare-changelogs.mjs';

const [version, date] = process.argv.slice(2);
const template = readFileSync(new URL('../.github/PULL_REQUEST_TEMPLATE.md', import.meta.url), 'utf8');
process.stdout.write(renderChangelogPullRequestBody({ version, date, template }));
