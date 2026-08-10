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

  createOrganization(attributes) {
    const organization = createOrganization(attributes);
    this.organizations.set(organization.id, organization);
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

    this.projects.set(project.id, project);
    return project;
  }

  getProject(id) {
    return this.projects.get(id) ?? null;
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
    this.repositories.set(repository.id, repository);
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
    this.proposals.set(proposal.id, proposal);
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
    this.proposals.set(id, updated);
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
    this.reviewComments.set(reviewComment.id, reviewComment);
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
    this.reviewComments.set(id, updated);
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
    this.workflowRuns.set(workflowRun.id, workflowRun);
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
    this.workflowRuns.set(id, updated);
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
    this.policies.set(policy.id, policy);
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
