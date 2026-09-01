export type Principal = {
  type?: string;
  id?: string;
  displayName?: string;
};

export type Project = {
  id?: string;
  name?: string;
  slug?: string;
  organizationId?: string;
  description?: string;
  status?: string;
  repositoryIds?: string[];
  principalRefs?: Principal[];
  metadata?: Record<string, unknown>;
  createdAt?: string;
  updatedAt?: string;
};

export type Organization = {
  id?: string;
  name?: string;
  slug?: string;
  ownerPrincipal?: Principal;
  principalRefs?: Principal[];
  metadata?: Record<string, unknown>;
  createdAt?: string;
  updatedAt?: string;
};

export type Repository = {
  id?: string;
  organizationId?: string;
  projectId?: string;
  provider?: string;
  owner?: string;
  name?: string;
  defaultBranch?: string;
  url?: string;
};

export type Proposal = {
  id?: string;
  projectId?: string;
  repositoryId?: string;
  syncRepoId?: string;
  title?: string;
  description?: string;
  status?: string;
  sourceLane?: string;
  targetLane?: string;
  sourceBranch?: string;
  targetBranch?: string;
  authorRef?: string;
  authorPrincipal?: Principal;
  createdAt?: string;
  updatedAt?: string;
  metadata?: Record<string, unknown>;
};

export type ReviewComment = {
  id?: string;
  proposalId?: string;
  body?: string;
  path?: string;
  state?: string;
  authorRef?: string;
  authorPrincipal?: Principal;
  createdAt?: string;
  updatedAt?: string;
};

export type WorkflowRun = {
  id?: string;
  projectId?: string;
  proposalId?: string;
  name?: string;
  status?: string;
  createdAt?: string;
  updatedAt?: string;
};

export type SyncRepo = { id?: string; refCount?: number };
export type SyncRef = { name?: string; snapshot?: string };

export type SnapshotSummary = {
  id: string;
  message: string | null;
  createdAt: string | null;
  author: Principal | null;
  parents: string[];
};

export type TreeEntry = {
  name: string;
  path: string;
  type: string;
  mode: string | null;
  size: number | null;
  objectId: string;
};

export type TreeResponse = {
  repoId: string;
  ref: string;
  path: string;
  snapshot: SnapshotSummary;
  entries: TreeEntry[];
};

export type TextFileResponse = {
  repoId: string;
  ref: string;
  path: string;
  objectId: string;
  size: number;
  encoding: 'utf-8';
  content: string;
  snapshot: SnapshotSummary;
};

export function metadataString(
  metadata: Record<string, unknown> | undefined,
  key: string,
): string | undefined {
  const value = metadata?.[key];
  return typeof value === 'string' && value.trim() ? value : undefined;
}

export function principalLabel(principal: Principal | null | undefined): string {
  return principal?.displayName ?? principal?.id ?? 'Unknown';
}

export function initials(value: string | null | undefined): string {
  const parts = (value ?? 'S').split(/[^A-Za-z0-9]+/).filter(Boolean);
  return parts.slice(0, 2).map((part) => part[0]?.toUpperCase()).join('') || 'S';
}

export function relativeTime(value: string | null | undefined): string {
  if (!value) return 'recently';
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) return value;
  const seconds = Math.round((timestamp - Date.now()) / 1000);
  const formatter = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
  if (Math.abs(seconds) < 60) return formatter.format(seconds, 'second');
  const minutes = Math.round(seconds / 60);
  if (Math.abs(minutes) < 60) return formatter.format(minutes, 'minute');
  const hours = Math.round(minutes / 60);
  if (Math.abs(hours) < 24) return formatter.format(hours, 'hour');
  return formatter.format(Math.round(hours / 24), 'day');
}
