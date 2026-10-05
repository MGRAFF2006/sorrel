import {
  createOrganization,
  createPolicy,
  createProject,
  createProposal,
  createRepository,
  createReviewComment,
  createWorkflowRun,
  updateProposal,
  updateReviewComment,
  updateWorkflowRun,
} from './models.js';
import { createRepoSyncStore } from './sync-store.js';

export class StoreConflictError extends Error {
  constructor(message) {
    super(message);
    this.name = 'StoreConflictError';
    this.code = 'store_conflict';
  }
}

export class StoreNotFoundError extends Error {
  constructor(message) {
    super(message);
    this.name = 'StoreNotFoundError';
    this.code = 'not_found';
  }
}

export class InMemoryStore {
  constructor(options = {}) {
    this.organizations = new Map();
    this.projects = new Map();
    this.repositories = new Map();
    this.proposals = new Map();
    this.reviewComments = new Map();
    this.workflowRuns = new Map();
    this.policies = new Map();
    this.sync = options.sync ?? createRepoSyncStore();
  }

  /** Publish a validated record. Persistent stores write it before publication. */
  storeRecord(collection, record) {
    this[collection].set(record.id, record);
  }

  createOrganization(attributes) {
    const organization = createOrganization(attributes);
    if (this.organizations.has(organization.id)) {
      throw new StoreConflictError(`organization ${organization.id} already exists`);
    }
    this.storeRecord('organizations', organization);
    return organization;
  }

  getOrganization(id) {
    return this.organizations.get(id) ?? null;
  }

  listOrganizations() {
    return [...this.organizations.values()];
  }

  createProject(attributes) {
    const project = createProject(attributes);
    const duplicate = this.listProjects({ organizationId: project.organizationId }).find(
      (existing) => existing.slug === project.slug,
    );

    if (duplicate) {
      throw new StoreConflictError('project slug already exists for organization');
    }

    if (this.projects.has(project.id)) {
      throw new StoreConflictError(`project ${project.id} already exists`);
    }
    this.storeRecord('projects', project);
    return project;
  }

  getProject(id) {
    return this.projects.get(id) ?? null;
  }

  linkProjectRepository(projectId, syncRepoId) {
    const project = this.getProject(projectId);
    if (!project) {
      throw new StoreNotFoundError(`project ${projectId} not found`);
    }
    const updated = {
      ...project,
      repositoryIds: [...new Set([...project.repositoryIds, syncRepoId])],
      updatedAt: new Date().toISOString(),
    };
    this.storeRecord('projects', updated);
    return updated;
  }

  listProjects(filters = {}) {
    return [...this.projects.values()].filter((project) => {
      if (filters.organizationId && project.organizationId !== filters.organizationId) {
        return false;
      }

      return true;
    });
  }

  createRepository(attributes) {
    const repository = createRepository(attributes);
    if (this.repositories.has(repository.id)) {
      throw new StoreConflictError(`repository ${repository.id} already exists`);
    }
    this.storeRecord('repositories', repository);
    return repository;
  }

  getRepository(id) {
    return this.repositories.get(id) ?? null;
  }

  listRepositories(filters = {}) {
    return [...this.repositories.values()].filter((repository) => {
      if (filters.organizationId && repository.organizationId !== filters.organizationId) {
        return false;
      }

      if (filters.projectId && repository.projectId !== filters.projectId) {
        return false;
      }

      return true;
    });
  }

  createProposal(attributes) {
    const proposal = createProposal(attributes);
    if (this.proposals.has(proposal.id)) {
      throw new StoreConflictError(`proposal ${proposal.id} already exists`);
    }
    this.storeRecord('proposals', proposal);
    return proposal;
  }

  getProposal(id) {
    return this.proposals.get(id) ?? null;
  }

  updateProposal(id, attributes) {
    const existing = this.getProposal(id);
    if (!existing) {
      throw new StoreNotFoundError(`proposal ${id} not found`);
    }
    const updated = updateProposal(existing, attributes);
    this.storeRecord('proposals', updated);
    return updated;
  }

  listProposals(filters = {}) {
    return [...this.proposals.values()].filter((proposal) => {
      if (filters.projectId && proposal.projectId !== filters.projectId) {
        return false;
      }

      if (filters.repositoryId && proposal.repositoryId !== filters.repositoryId) {
        return false;
      }

      if (filters.syncRepoId && proposal.syncRepoId !== filters.syncRepoId) {
        return false;
      }

      if (filters.status && proposal.status !== filters.status) {
        return false;
      }

      if (filters.sourceLane && proposal.sourceLane !== filters.sourceLane) {
        return false;
      }

      return true;
    });
  }

  createReviewComment(attributes) {
    const reviewComment = createReviewComment(attributes);
    if (!this.getProposal(reviewComment.proposalId)) {
      throw new StoreNotFoundError(`proposal ${reviewComment.proposalId} not found`);
    }
    if (this.reviewComments.has(reviewComment.id)) {
      throw new StoreConflictError(`reviewComment ${reviewComment.id} already exists`);
    }
    this.storeRecord('reviewComments', reviewComment);
    return reviewComment;
  }

  getReviewComment(id) {
    return this.reviewComments.get(id) ?? null;
  }

  updateReviewComment(id, attributes) {
    const existing = this.getReviewComment(id);
    if (!existing) {
      throw new StoreNotFoundError(`review comment ${id} not found`);
    }
    const updated = updateReviewComment(existing, attributes);
    this.storeRecord('reviewComments', updated);
    return updated;
  }

  listReviewComments(filters = {}) {
    return [...this.reviewComments.values()].filter((reviewComment) => {
      if (filters.proposalId && reviewComment.proposalId !== filters.proposalId) {
        return false;
      }

      if (filters.state && reviewComment.state !== filters.state) {
        return false;
      }

      return true;
    });
  }

  createWorkflowRun(attributes) {
    const workflowRun = createWorkflowRun(attributes);
    if (this.workflowRuns.has(workflowRun.id)) {
      throw new StoreConflictError(`workflowRun ${workflowRun.id} already exists`);
    }
    this.storeRecord('workflowRuns', workflowRun);
    return workflowRun;
  }

  getWorkflowRun(id) {
    return this.workflowRuns.get(id) ?? null;
  }

  updateWorkflowRun(id, attributes) {
    const existing = this.getWorkflowRun(id);
    if (!existing) {
      throw new StoreNotFoundError(`workflow run ${id} not found`);
    }
    const updated = updateWorkflowRun(existing, attributes);
    this.storeRecord('workflowRuns', updated);
    return updated;
  }

  listWorkflowRuns(filters = {}) {
    return [...this.workflowRuns.values()].filter((workflowRun) => {
      if (filters.projectId && workflowRun.projectId !== filters.projectId) {
        return false;
      }

      if (filters.proposalId && workflowRun.proposalId !== filters.proposalId) {
        return false;
      }

      if (filters.status && workflowRun.status !== filters.status) {
        return false;
      }

      return true;
    });
  }

  createPolicy(attributes) {
    const policy = createPolicy(attributes);
    if (this.policies.has(policy.id)) {
      throw new StoreConflictError(`policy ${policy.id} already exists`);
    }
    this.storeRecord('policies', policy);
    return policy;
  }

  getPolicy(id) {
    return this.policies.get(id) ?? null;
  }

  listPolicies(filters = {}) {
    return [...this.policies.values()].filter((policy) => {
      if (filters.organizationId && policy.organizationId !== filters.organizationId) {
        return false;
      }

      if (filters.projectId && policy.projectId !== filters.projectId) {
        return false;
      }

      return true;
    });
  }
}

export function createInMemoryStore(options = {}) {
  return new InMemoryStore(options);
}
