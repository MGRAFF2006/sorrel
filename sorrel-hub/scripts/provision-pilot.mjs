import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { createFsMetadataStore } from '../src/fs-metadata-store.js';
import { atomicWrite } from '../src/fs-sync-store.js';

/** Operator-owned Core records, scoped to one invited subject and workspace. */
export function pilotGrants({ subject, organization, project, repository }) {
  for (const [name, value] of Object.entries({ subject, organization, project, repository })) {
    if (typeof value !== 'string' || !/^[a-zA-Z0-9_:-]+$/.test(value) || value.includes('*')) throw new Error(`${name} must be a concrete identifier`);
  }
  if (!subject.startsWith('user_')) throw new Error('subject must be the WorkOS user id (user_…), not an email address');
  const principal = { kind: 'user', id: `workos:${subject}` };
  return Object.fromEntries([
    ['org', organization, ['org.read']],
    ['project', project, ['project.read', 'project.write', 'proposal.read', 'proposal.write', 'proposal.review', 'review.comment.read', 'review.comment.write', 'workflow.run.read']],
    ['repo', repository, ['repo.read', 'repo.object.write', 'repo.ref.write']],
  ].map(([kind, id, capabilities]) => {
    const grantId = `grant_pilot_${createHash('sha256').update(`${subject}:${kind}:${id}`).digest('hex').slice(0, 20)}`;
    return [grantId, { schemaVersion: 'sorrel.protocol.v0', kind: 'Grant', id: grantId, principal,
      resource: { kind, id }, capabilities, effect: 'allow' }];
  }));
}

export function provisionPilot(options) {
  const grants = pilotGrants(options);
  const grantsPath = resolve(options.grantsFile);
  const existing = existsSync(grantsPath) ? JSON.parse(readFileSync(grantsPath, 'utf8')) : {};
  if (!existing || typeof existing !== 'object' || Array.isArray(existing)) throw new Error('grants file must be a JSON object');
  for (const [id, grant] of Object.entries(grants)) {
    if (Object.hasOwn(existing, id) && JSON.stringify(existing[id]) !== JSON.stringify(grant)) throw new Error(`refusing to replace changed authority record ${id}`);
  }
  const records = {
    organization: { id: options.organization, name: options.organization },
    project: { id: options.project, organizationId: options.organization, name: options.name ?? options.project, repositoryIds: [options.repository] },
    repository: { id: options.repository, projectId: options.project, organizationId: options.organization,
      provider: 'sorrel', owner: options.organization, name: options.repository, defaultBranch: 'HEAD' },
  };
  if (options.write) {
    const store = createFsMetadataStore(resolve(options.metadataDir));
    for (const [kind, record] of Object.entries(records)) {
      const existingRecord = store[kind === 'repository' ? 'repositories' : `${kind}s`].get(record.id);
      if (existingRecord && (kind === 'project' ? existingRecord.organizationId !== record.organizationId || !existingRecord.repositoryIds?.includes(options.repository)
        : kind === 'repository' ? existingRecord.projectId !== record.projectId || existingRecord.organizationId !== record.organizationId : false)) {
        throw new Error(`existing ${kind} belongs to another workspace`);
      }
    }
    if (!store.organizations.has(records.organization.id)) store.createOrganization(records.organization);
    if (!store.projects.has(records.project.id)) store.createProject(records.project);
    if (!store.repositories.has(records.repository.id)) store.createRepository(records.repository);
    mkdirSync(dirname(grantsPath), { recursive: true });
    atomicWrite(grantsPath, `${JSON.stringify({ ...existing, ...grants }, null, 2)}\n`, dirname(grantsPath));
  }
  return { principal: `user:workos:${options.subject}`, records, grants, written: Boolean(options.write) };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const { values } = parseArgs({ options: {
      subject: { type: 'string' }, organization: { type: 'string' }, project: { type: 'string' },
      repository: { type: 'string' }, name: { type: 'string' }, 'metadata-dir': { type: 'string' },
      'grants-file': { type: 'string' }, write: { type: 'boolean', default: false },
    } });
    if (!values['metadata-dir'] || !values['grants-file']) throw new Error('provide --metadata-dir and --grants-file; stop Hub before using --write, then restart it');
    const result = provisionPilot({ ...values, metadataDir: values['metadata-dir'], grantsFile: values['grants-file'] });
    console.log(JSON.stringify(result, null, 2));
  } catch (error) {
    console.error(`provision-pilot: ${error.message}`);
    process.exitCode = 1;
  }
}
