import type { Principal } from '@sorrel/sdk-js';

export type { Principal };

export type Connection = {
  baseUrl: string;
  principal: Principal;
};

export type HubCapabilities = {
  modules: {
    core: boolean;
    actions: boolean;
    agents: boolean;
    secrets: boolean;
    objectStorage: 'fs' | 'memory';
  };
  auth: {
    mode: 'dev' | 'workos' | 'oidc';
    session: 'cookie' | 'bearer' | 'none';
  };
  convex: { enabled: boolean; url?: string };
  deploy: 'saas' | 'selfhost' | 'dev';
};

export type HubSession = {
  auth: HubCapabilities['auth'];
  session: {
    sessionId: string;
    authMode: 'dev' | 'workos' | 'oidc';
    principal: Principal;
    idpSubject: string | null;
    expiresAt: number | null;
  } | null;
};

export type Project = {
  id: string;
  name?: string;
  organizationId?: string;
  description?: string;
  status?: string;
  slug?: string;
  repositoryIds?: string[];
  policyRefs?: unknown[];
  grantRefs?: unknown[];
};

export type Proposal = {
  id: string;
  title?: string;
  description?: string;
  status?: string;
  projectId?: string;
  syncRepoId?: string;
  repositoryId?: string;
  sourceLane?: string;
  targetLane?: string;
  sourceSnapshot?: string;
  authorRef?: string;
  createdAt?: number | string;
  updatedAt?: number | string;
  comments?: ReviewComment[];
};

export type ReviewComment = {
  id: string;
  proposalId?: string;
  body?: string;
  state?: string;
  path?: string;
  line?: number;
  authorRef?: string;
  createdAt?: number | string;
};

export type SyncRepo = { id: string; refCount?: number };
export type SyncRef = { name?: string; snapshot?: string };
export type Repository = {
  id: string;
  name?: string;
  owner?: string;
  provider?: string;
  projectId?: string;
};

export type ProposalSummary = {
  total?: number;
  byStatus?: Record<string, number>;
};

export type DataResponse<T> = { data: T };
export type RepoListResponse = { repos: SyncRepo[] };
export type RefListResponse = { refs: SyncRef[] } | { data: SyncRef[] };
