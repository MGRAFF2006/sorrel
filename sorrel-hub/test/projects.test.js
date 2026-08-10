import assert from 'node:assert/strict';
import http from 'node:http';
import test from 'node:test';

import { createApp } from '../src/app.js';

async function withServer(callback) {
  const app = createApp();
  const server = http.createServer(app.handleRequest);

  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));

  const address = server.address();
  const baseUrl = `http://${address.address}:${address.port}`;

  try {
    return await callback(baseUrl);
  } finally {
    await new Promise((resolve, reject) => {
      server.close((error) => (error ? reject(error) : resolve()));
    });
  }
}

async function postJson(url, payload, headers = {}) {
  return await fetch(url, {
    method: 'POST',
    headers: {
      'content-type': 'application/json',
      ...headers,
    },
    body: JSON.stringify(payload),
  });
}

const maintainerGrant = {
  id: 'grant_repo_maintainer',
  source: 'core',
  principal: { type: 'user', id: 'user_maintainer' },
  action: 'policy.grant',
  resource: { kind: 'org', id: 'org_policy' },
};

async function withPolicyServer(callback) {
  const app = createApp({
    trustedGrantsById: {
      [maintainerGrant.id]: maintainerGrant,
    },
  });
  const server = http.createServer(app.handleRequest);

  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));

  const address = server.address();
  const baseUrl = `http://${address.address}:${address.port}`;

  try {
    return await callback(baseUrl);
  } finally {
    await new Promise((resolve, reject) => {
      server.close((error) => (error ? reject(error) : resolve()));
    });
  }
}

test('GET /healthz returns service health', async () => {
  await withServer(async (baseUrl) => {
    const response = await fetch(`${baseUrl}/healthz`);
    const body = await response.json();

    assert.equal(response.status, 200);
    assert.deepEqual(body, {
      status: 'ok',
      service: 'sorrel-hub',
    });
  });
});

test('GET /projects returns an empty collection initially', async () => {
  await withServer(async (baseUrl) => {
    const response = await fetch(`${baseUrl}/projects`);
    const body = await response.json();

    assert.equal(response.status, 200);
    assert.deepEqual(body, { data: [] });
  });
});

test('POST /projects creates a project and GET /projects lists it', async () => {
  await withServer(async (baseUrl) => {
    const createResponse = await postJson(`${baseUrl}/projects`, {
      organizationId: 'org_local',
      name: 'Platform Collaboration',
      description: 'Initial Sorrel Hub collaboration project',
    });
    const created = await createResponse.json();

    assert.equal(createResponse.status, 201);
    assert.match(createResponse.headers.get('location'), /^\/projects\/proj_/);
    assert.equal(created.data.organizationId, 'org_local');
    assert.equal(created.data.name, 'Platform Collaboration');
    assert.equal(created.data.slug, 'platform-collaboration');
    assert.equal(created.data.status, 'active');

    const listResponse = await fetch(`${baseUrl}/projects`);
    const listed = await listResponse.json();

    assert.equal(listResponse.status, 200);
    assert.equal(listed.data.length, 1);
    assert.equal(listed.data[0].id, created.data.id);
  });
});

test('GET /projects filters by organizationId', async () => {
  await withServer(async (baseUrl) => {
    await postJson(`${baseUrl}/projects`, { organizationId: 'org_a', name: 'Project A' });
    await postJson(`${baseUrl}/projects`, { organizationId: 'org_b', name: 'Project B' });

    const response = await fetch(`${baseUrl}/projects?organizationId=org_b`);
    const body = await response.json();

    assert.equal(response.status, 200);
    assert.equal(body.data.length, 1);
    assert.equal(body.data[0].organizationId, 'org_b');
  });
});

test('POST /projects validates required fields', async () => {
  await withServer(async (baseUrl) => {
    const response = await postJson(`${baseUrl}/projects`, { organizationId: 'org_local' });
    const body = await response.json();

    assert.equal(response.status, 400);
    assert.deepEqual(body, {
      error: {
        code: 'model_validation_failed',
        message: 'name is required',
      },
    });
  });
});

test('POST /projects rejects duplicate slugs within an organization', async () => {
  await withServer(async (baseUrl) => {
    const payload = { organizationId: 'org_local', name: 'Policy Engine' };
    await postJson(`${baseUrl}/projects`, payload);

    const response = await postJson(`${baseUrl}/projects`, payload);
    const body = await response.json();

    assert.equal(response.status, 409);
    assert.deepEqual(body, {
      error: {
        code: 'store_conflict',
        message: 'project slug already exists for organization',
      },
    });
  });
});

test('POST /projects records Core policy references', async () => {
  await withServer(async (baseUrl) => {
    const policyRefs = [
      { kind: 'Policy', id: 'policy_project_admin' },
      { kind: 'AgentPolicy', id: 'agent_policy_ci' },
    ];
    const coreRefs = {
      grantRefs: [{ id: 'grant_project_admin', source: 'core' }],
      policyDecisionRefs: [{ id: 'decision_project_create', source: 'core' }],
      auditEventRefs: [{ id: 'audit_project_create', source: 'core' }],
    };

    const createResponse = await postJson(`${baseUrl}/projects`, {
      organizationId: 'org_policy',
      name: 'Policy References',
      createdByPrincipal: { type: 'user', id: 'user_alice', displayName: 'Alice' },
      policyRefs,
      ...coreRefs,
    });
    const created = await createResponse.json();

    assert.equal(createResponse.status, 201);
    assert.deepEqual(created.data.createdByPrincipal, {
      type: 'user',
      id: 'user_alice',
      displayName: 'Alice',
    });
    assert.deepEqual(created.data.policyRefs, policyRefs);
    assert.deepEqual(created.data.grantRefs, coreRefs.grantRefs);
    assert.deepEqual(created.data.policyDecisionRefs, coreRefs.policyDecisionRefs);
    assert.deepEqual(created.data.auditEventRefs, coreRefs.auditEventRefs);

    const listResponse = await fetch(`${baseUrl}/projects?organizationId=org_policy`);
    const listed = await listResponse.json();

    assert.equal(listResponse.status, 200);
    assert.deepEqual(listed.data[0].policyRefs, policyRefs);
  });
});

test('POST /admin/proposals records policy references without merge queue behavior', async () => {
  await withServer(async (baseUrl) => {
    const policyRefs = [{ kind: 'Policy', id: 'policy_proposal_review' }];

    const response = await postJson(`${baseUrl}/admin/proposals`, {
      projectId: 'proj_policy',
      repositoryId: 'repo_policy',
      title: 'Review policy metadata',
      authorPrincipal: { type: 'user', id: 'user_reviewer' },
      policyRefs,
      grantRefs: [{ id: 'grant_proposal_review', source: 'core' }],
      policyDecisionRefs: [{ id: 'decision_proposal_open', source: 'core' }],
      auditEventRefs: [{ id: 'audit_proposal_open', source: 'core' }],
    });
    const created = await response.json();

    assert.equal(response.status, 201);
    assert.match(response.headers.get('location'), /^\/admin\/proposals\/prop_/);
    assert.equal(created.data.authorRef, 'user:user_reviewer');
    assert.deepEqual(created.data.policyRefs, policyRefs);
    assert.deepEqual(created.data.policyDecisionRefs, [
      { id: 'decision_proposal_open', source: 'core' },
    ]);
  });
});

test('POST /admin/workflow-runs records policy references without hosted compute', async () => {
  await withServer(async (baseUrl) => {
    const policyRefs = [{ kind: 'AgentPolicy', id: 'agent_policy_workflow_ci' }];

    const response = await postJson(`${baseUrl}/admin/workflow-runs`, {
      projectId: 'proj_policy',
      proposalId: 'prop_policy',
      name: 'CI',
      requestedByPrincipal: { type: 'agent', id: 'agent_review_bot' },
      runnerPrincipal: { type: 'runner', id: 'runner_local' },
      policyRefs,
      grantRefs: [{ id: 'grant_workflow_run', source: 'core' }],
      policyDecisionRefs: [{ id: 'decision_workflow_run', source: 'core' }],
      auditEventRefs: [{ id: 'audit_workflow_run', source: 'core' }],
    });
    const created = await response.json();

    assert.equal(response.status, 201);
    assert.match(response.headers.get('location'), /^\/admin\/workflow-runs\/run_/);
    assert.deepEqual(created.data.requestedByPrincipal, {
      type: 'agent',
      id: 'agent_review_bot',
    });
    assert.deepEqual(created.data.runnerPrincipal, {
      type: 'runner',
      id: 'runner_local',
    });
    assert.deepEqual(created.data.policyRefs, policyRefs);
    assert.deepEqual(created.data.auditEventRefs, [{ id: 'audit_workflow_run', source: 'core' }]);
  });
});

test('POST /admin/policies rejects Hub-local authorization rules', async () => {
  await withPolicyServer(async (baseUrl) => {
    const response = await postJson(
      `${baseUrl}/admin/policies`,
      {
        organizationId: 'org_policy',
        name: 'Project access',
        policyRef: { kind: 'Policy', id: 'policy_project_access' },
        grantRefs: [{ id: maintainerGrant.id, source: 'core' }],
        rules: [{ type: 'approval', enforcement: 'required' }],
      },
      {
        'x-sorrel-acting-principal': JSON.stringify(maintainerGrant.principal),
      },
    );
    const body = await response.json();

    assert.equal(response.status, 400);
    assert.deepEqual(body, {
      error: {
        code: 'model_validation_failed',
        message: 'rules on policy is owned by Core/protocol; reference Core policy and grant records instead',
      },
    });
  });
});

test('POST /admin/repositories exposes Core policy references', async () => {
  await withPolicyServer(async (baseUrl) => {
    const policyRef = { kind: 'Policy', id: 'policy_repo_access' };
    const authorityRootRef = { kind: 'AuthorityRoot', id: 'authority_org_policy' };
    const policyRefs = [{ kind: 'AgentPolicy', id: 'agent_policy_repo_ci' }];
    const grantRefs = [{ id: maintainerGrant.id, source: 'core' }];

    const response = await postJson(
      `${baseUrl}/admin/repositories`,
      {
        organizationId: 'org_policy',
        projectId: 'proj_policy',
        provider: 'sorrel',
        owner: 'acme',
        name: 'platform',
        policyRef,
        authorityRootRef,
        policyRefs,
        grantRefs,
      },
      {
        'x-sorrel-acting-principal': JSON.stringify(maintainerGrant.principal),
      },
    );
    const created = await response.json();

    assert.equal(response.status, 201);
    assert.match(response.headers.get('location'), /^\/admin\/repositories\/repo_/);
    assert.deepEqual(created.data.policyRef, policyRef);
    assert.deepEqual(created.data.authorityRootRef, authorityRootRef);
    assert.deepEqual(created.data.policyRefs, policyRefs);
    assert.deepEqual(created.data.grantRefs, grantRefs);

    const listResponse = await fetch(`${baseUrl}/admin/repositories?organizationId=org_policy`);
    const listed = await listResponse.json();

    assert.equal(listResponse.status, 200);
    assert.deepEqual(listed.data[0].policyRef, policyRef);
    assert.deepEqual(listed.data[0].authorityRootRef, authorityRootRef);
  });
});

test('POST /admin/repositories denies unauthorized agents for policy.grant', async () => {
  await withPolicyServer(async (baseUrl) => {
    const response = await postJson(
      `${baseUrl}/admin/repositories`,
      {
        organizationId: 'org_policy',
        projectId: 'proj_policy',
        provider: 'sorrel',
        owner: 'acme',
        name: 'restricted',
        policyRef: { kind: 'Policy', id: 'policy_repo_access' },
        authorityRootRef: { kind: 'AuthorityRoot', id: 'authority_org_policy' },
        grantRefs: [{ id: maintainerGrant.id, source: 'core' }],
      },
      {
        'x-sorrel-acting-principal': JSON.stringify({ type: 'agent', id: 'agent_unauthorized' }),
      },
    );
    const body = await response.json();

    assert.equal(response.status, 403);
    assert.equal(body.error.code, 'policy_denied');
    assert.equal(body.error.decision.outcome, 'deny');
  });
});

test('POST /admin/repositories allows maintainers via hydrated Core grants', async () => {
  await withPolicyServer(async (baseUrl) => {
    const response = await postJson(
      `${baseUrl}/admin/repositories`,
      {
        organizationId: 'org_policy',
        projectId: 'proj_policy',
        provider: 'sorrel',
        owner: 'acme',
        name: 'maintained',
        policyRef: { kind: 'Policy', id: 'policy_repo_access' },
        authorityRootRef: { kind: 'AuthorityRoot', id: 'authority_org_policy' },
        grantRefs: [{ id: maintainerGrant.id, source: 'core' }],
      },
      {
        'x-sorrel-acting-principal': JSON.stringify(maintainerGrant.principal),
      },
    );
    const created = await response.json();

    assert.equal(response.status, 201);
    assert.equal(created.data.name, 'maintained');
    assert.deepEqual(created.data.grantRefs, [{ id: maintainerGrant.id, source: 'core' }]);
  });
});
