import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';
import { encodePathSegment } from '../src/fs-sync-store.js';

function fixture(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'sorrel-metadata-identity-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const warnings = [];
  t.mock.method(console, 'warn', (...args) => warnings.push(args.join(' ')));
  return { dir, store: createFsMetadataStore(dir), warnings };
}

function write(dir, collection, filename, value) {
  fs.mkdirSync(path.join(dir, collection), { recursive: true });
  fs.writeFileSync(path.join(dir, collection, filename), JSON.stringify(value));
}

test('misnamed metadata can never overwrite or create an alternate record identity at startup', t => {
  const { dir, store, warnings } = fixture(t);
  const project = store.createProject({ id: 'project_A', organizationId: 'org_fixture', name: 'Original' });
  const validFilename = path.join(dir, 'projects', 'project_A.json');
  const validBytes = fs.readFileSync(validFilename, 'utf8');
  write(dir, 'projects', 'project_B.json', { ...project, name: 'Wrong identity' });
  write(dir, 'projects', 'project_C.json', { ...project, id: 'project_missing', name: 'Orphan alias' });
  const aliasBytes = fs.readFileSync(path.join(dir, 'projects', 'project_B.json'), 'utf8');
  const reopened = createFsMetadataStore(dir);
  assert.deepEqual(reopened.getProject(project.id), JSON.parse(validBytes));
  assert.equal(reopened.getProject('project_missing'), null);
  assert.equal(reopened.getProject('project_B'), null);
  assert.equal(reopened.listProjects().length, 1);
  assert.equal(fs.readFileSync(validFilename, 'utf8'), validBytes);
  assert.equal(fs.readFileSync(path.join(dir, 'projects', 'project_B.json'), 'utf8'), aliasBytes);
  assert.equal(warnings.filter(message => message.includes('filename/identity mismatch')).length, 2);
  // The canonical record remains usable, unlike a colliding malformed alias.
  assert.deepEqual(reopened.linkProjectRepository(project.id, 'repo_fixture').repositoryIds, ['repo_fixture']);
});

test('canonical punctuation, Unicode and percent-looking IDs round-trip across restart', t => {
  const { dir, store, warnings } = fixture(t);
  for (const [index, id] of ['project/ü?part#one', '组织项目', 'project%2f', 'with space', '\ufffd'].entries()) {
    const created = store.createProject({ id, organizationId: 'org_fixture', name: `Project ${index}` });
    assert.ok(fs.existsSync(path.join(dir, 'projects', `${encodePathSegment(id)}.json`)));
    assert.deepEqual(createFsMetadataStore(dir).getProject(id), JSON.parse(JSON.stringify(created)));
  }
  assert.deepEqual(warnings, []);
});

test('noncanonical alias encodings and malformed IDs are skipped without rewriting files', t => {
  const { dir, store, warnings } = fixture(t);
  const project = store.createProject({ id: 'project/canonical', organizationId: 'org_fixture', name: 'Original' });
  const cases = [
    ['project%2Fcanonical.json', { ...project, name: 'Uppercase percent alias' }],
    ['%70roject%2fcanonical.json', { ...project, name: 'Overencoded alias' }],
    ['bad%zz.json', { ...project, id: 'bad%zz' }],
    ['empty.json', { ...project, id: '' }],
    ['%.json', { ...project, id: '' }],
    ['%2e.json', { ...project, id: '.' }],
    ['%2e%2e.json', { ...project, id: '..' }],
    ['%20space.json', { ...project, id: ' space' }],
    ['surrogate.json', { ...project, id: '\ud800' }],
    [`${encodePathSegment('\ufffd')}.json`, { ...project, id: '\udc00' }],
  ];
  for (const [filename, value] of cases) write(dir, 'projects', filename, value);
  const before = Object.fromEntries(fs.readdirSync(path.join(dir, 'projects')).map(filename => [filename, fs.readFileSync(path.join(dir, 'projects', filename), 'utf8')]));
  const reopened = createFsMetadataStore(dir);
  assert.deepEqual(reopened.listProjects(), [JSON.parse(JSON.stringify(project))]);
  const after = Object.fromEntries(fs.readdirSync(path.join(dir, 'projects')).map(filename => [filename, fs.readFileSync(path.join(dir, 'projects', filename), 'utf8')]));
  assert.deepEqual(after, before);
  assert.equal(warnings.length, cases.length);
});

test('identity guard covers every metadata collection and preserves opaque extension data', t => {
  const { dir, warnings } = fixture(t);
  for (const collection of ['organizations', 'projects', 'repositories', 'proposals', 'reviewComments', 'workflowRuns', 'policies']) {
    const value = { id: `valid_${collection}`, opaqueExtension: { unknown: [null, 42, false] } };
    write(dir, collection, `${encodePathSegment(value.id)}.json`, value);
    write(dir, collection, 'alias.json', { ...value, opaqueExtension: 'must not replace original' });
    assert.deepEqual(createFsMetadataStore(dir)[collection].get(value.id), value);
  }
  assert.ok(warnings.every(message => message.includes('filename/identity mismatch')));
});

test('corrupt JSON diagnostics never include record payloads or secret-like contents', t => {
  const { dir, warnings } = fixture(t);
  fs.mkdirSync(path.join(dir, 'projects'), { recursive: true });
  fs.writeFileSync(path.join(dir, 'projects', 'corrupt.json'), '{"private":"synthetic-do-not-log-metadata"');
  assert.doesNotThrow(() => createFsMetadataStore(dir));
  assert.equal(warnings.length, 1);
  assert.match(warnings[0], /skipping corrupt record/);
  assert.equal(warnings[0].includes('synthetic-do-not-log-metadata'), false);
});
