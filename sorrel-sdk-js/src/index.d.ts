export type Principal = { type: string; id: string };

export type HubClientOptions = {
  baseUrl: string;
  principal?: Principal;
  accessToken?: string;
  fetch?: typeof globalThis.fetch;
};

export type ProjectQuery = { organizationId?: string };
export type ProposalQuery = {
  projectId?: string;
  repositoryId?: string;
  syncRepoId?: string;
  status?: string;
  sourceLane?: string;
};

export declare class HubClient {
  readonly baseUrl: string;
  readonly principal: Principal;
  readonly accessToken?: string;

  constructor(options: HubClientOptions);

  request<T = unknown>(path: string, init?: RequestInit): Promise<T>;
  health<T = unknown>(): Promise<T>;
  capabilities<T = unknown>(): Promise<T>;
  session<T = unknown>(): Promise<T>;
  listProjects<T = unknown>(query?: ProjectQuery): Promise<T>;
  getProject<T = unknown>(id: string): Promise<T>;
  createProject<T = unknown>(payload: unknown): Promise<T>;
  listSyncRepos<T = unknown>(): Promise<T>;
  listRefs<T = unknown>(repoId: string): Promise<T>;
  listAdminCollection<T = unknown>(name: string): Promise<T>;
  listRepositories<T = unknown>(query?: {
    organizationId?: string;
    projectId?: string;
  }): Promise<T>;
  listProposals<T = unknown>(query?: ProposalQuery): Promise<T>;
  getProposal<T = unknown>(
    id: string,
    options?: { includeComments?: boolean },
  ): Promise<T>;
  createProposal<T = unknown>(payload: unknown): Promise<T>;
  updateProposal<T = unknown>(id: string, payload: unknown): Promise<T>;
  createReviewComment<T = unknown>(payload: unknown): Promise<T>;
  updateReviewComment<T = unknown>(id: string, payload: unknown): Promise<T>;
  laneSubmit<T = unknown>(payload: unknown): Promise<T>;
  proposalSummary<T = unknown>(
    query?: Pick<ProposalQuery, 'projectId' | 'syncRepoId'>,
  ): Promise<T>;
}
