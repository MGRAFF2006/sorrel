import assert from 'node:assert/strict';
import http from 'node:http';
import test from 'node:test';

import { createApp } from '../src/app.js';

async function withServer(callback, options = {}) {
  const app = createApp(options);
  const server = http.createServer(app.handleRequest);

  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));

  const address = server.address();
  const baseUrl = `http://${address.address}:${address.port}`;

  try {
    return await callback(baseUrl, app);
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

async function patchJson(url, payload) {
  return await fetch(url, {
    method: 'PATCH',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(payload),
  });
}

test('full proposal lifecycle: create, get, comment, status transitions', async () => {
  await withServer(async (baseUrl) => {
    const projectRes = await postJson(`${baseUrl}/projects`, {
      organizationId: 'org_collab',
      name: 'Collab Project',
    });
    const project = (await projectRes.json()).data;
    assert.equal(projectRes.status, 201);

    const proposalRes = await postJson(`${baseUrl}/admin/proposals`, {
      projectId: project.id,
      syncRepoId: 'repo_sync_abc',
      title: 'Land feature lane',
      authorPrincipal: { type: 'user', id: 'local' },
      sourceLane: 'lane_feature',
      targetLane: 'lane_main',
      sourceSnapshot: 'aa'.repeat(32),
      status: 'draft',
    });
    const proposal = (await proposalRes.json()).data;
    assert.equal(proposalRes.status, 201);
    assert.equal(proposal.sourceLane, 'lane_feature');
    assert.equal(proposal.syncRepoId, 'repo_sync_abc');
    assert.equal(proposal.status, 'draft');

    const getRes = await fetch(`${baseUrl}/admin/proposals/${proposal.id}`);
    const got = await getRes.json();
    assert.equal(getRes.status, 200);
    assert.equal(got.data.id, proposal.id);

    const openRes = await patchJson(`${baseUrl}/admin/proposals/${proposal.id}`, {
      status: 'open',
    });
    assert.equal(openRes.status, 200);
    assert.equal((await openRes.json()).data.status, 'open');

    const commentRes = await postJson(`${baseUrl}/admin/review-comments`, {
      proposalId: proposal.id,
      body: 'Looks good — please add a test.',
      path: 'src/main.rs',
      line: 10,
      authorPrincipal: { type: 'user', id: 'reviewer' },
    });
    const comment = (await commentRes.json()).data;
    assert.equal(commentRes.status, 201);
    assert.equal(comment.proposalId, proposal.id);
    assert.equal(comment.state, 'open');

    const nested = await fetch(
      `${baseUrl}/admin/proposals/${proposal.id}?include=comments`,
    ).then((r) => r.json());
    assert.equal(nested.data.comments.length, 1);
    assert.equal(nested.data.comments[0].id, comment.id);

    const commentsOnly = await fetch(
      `${baseUrl}/admin/proposals/${proposal.id}/comments`,
    ).then((r) => r.json());
    assert.equal(commentsOnly.data.length, 1);

    const resolveRes = await patchJson(`${baseUrl}/admin/review-comments/${comment.id}`, {
      state: 'resolved',
    });
    assert.equal(resolveRes.status, 200);
    assert.equal((await resolveRes.json()).data.state, 'resolved');

    const approveRes = await patchJson(`${baseUrl}/admin/proposals/${proposal.id}`, {
      status: 'approved',
    });
    assert.equal((await approveRes.json()).data.status, 'approved');

    const mergeRes = await patchJson(`${baseUrl}/admin/proposals/${proposal.id}`, {
      status: 'merged',
    });
    assert.equal((await mergeRes.json()).data.status, 'merged');

    const badTransition = await patchJson(`${baseUrl}/admin/proposals/${proposal.id}`, {
      status: 'draft',
    });
    assert.equal(badTransition.status, 400);
  });
});

test('approved and merged proposal inputs remain bound to their review', async () => {
  await withServer(async (baseUrl, app) => {
    app.store.createProject({ id: 'proj_review', organizationId: 'org_local', name: 'Review' });
    app.store.createRepository({ id: 'repo_product', projectId: 'proj_review', organizationId: 'org_local', provider: 'sorrel', owner: 'local', name: 'Reviewed repository' });
    const inputs = {
      repositoryId: 'repo_product',
      syncRepoId: 'repo_sync',
      sourceBranch: 'feature',
      targetBranch: 'main',
      sourceLane: 'lane_feature',
      targetLane: 'lane_main',
      sourceSnapshot: 'aa'.repeat(32),
      targetSnapshot: 'bb'.repeat(32),
    };
    for (const status of ['approved', 'merged']) {
      const proposal = app.store.createProposal({
        projectId: 'proj_review', title: 'Reviewed change',
        authorPrincipal: { type: 'user', id: 'local' }, status, ...inputs,
      });
      const url = `${baseUrl}/admin/proposals/${proposal.id}`;
      for (const field of Object.keys(inputs)) {
        for (const value of ['replacement', null]) {
          const response = await patchJson(url, { [field]: value });
          assert.equal(response.status, 400, `${status}: ${field} = ${value}`);
          assert.equal((await response.json()).error.code, 'model_validation_failed');
          assert.deepEqual(app.store.getProposal(proposal.id), proposal);
        }
      }
      const transition = status === 'approved' ? 'open' : 'closed';
      const combined = await patchJson(url, {
        status: transition, sourceSnapshot: 'cc'.repeat(32),
      });
      assert.equal(combined.status, 400);
      assert.deepEqual(app.store.getProposal(proposal.id), proposal);

      const editable = await patchJson(url, {
        title: 'Clarified title', description: 'Clarified description',
        ...Object.fromEntries(Object.entries(inputs).map(([field, value]) => [field, ` ${value} `])),
      });
      assert.equal(editable.status, 200);
      const updated = (await editable.json()).data;
      assert.equal(updated.title, 'Clarified title');
      assert.equal(updated.description, 'Clarified description');
      assert.equal(updated.status, status);
      for (const [field, value] of Object.entries(inputs)) assert.equal(updated[field], value);
    }
  });
});

test('open proposal inputs can change, with an explicit reopen before replacing approved inputs', async () => {
  await withServer(async (baseUrl, app) => {
    app.store.createProject({ id: 'proj_review', organizationId: 'org_local', name: 'Review' });
    const proposal = app.store.createProposal({
      projectId: 'proj_review', title: 'Open change', status: 'open',
      authorPrincipal: { type: 'user', id: 'local' }, sourceSnapshot: 'aa'.repeat(32),
    });
    const url = `${baseUrl}/admin/proposals/${proposal.id}`;
    const edited = await patchJson(url, { sourceSnapshot: 'bb'.repeat(32), targetLane: 'lane_main' });
    assert.equal(edited.status, 200);
    assert.equal((await edited.json()).data.sourceSnapshot, 'bb'.repeat(32));
    for (const status of ['approved', 'merged']) {
      const before = structuredClone(app.store.getProposal(proposal.id));
      const combined = await patchJson(url, { status, sourceSnapshot: 'cc'.repeat(32) });
      assert.equal(combined.status, 400);
      assert.deepEqual(app.store.getProposal(proposal.id), before);
    }
    assert.equal((await patchJson(url, { status: 'approved' })).status, 200);
    assert.equal((await patchJson(url, { status: 'open' })).status, 200);
    assert.equal((await patchJson(url, { sourceSnapshot: 'cc'.repeat(32) })).status, 200);
    assert.equal(app.store.getProposal(proposal.id).status, 'open');
    assert.equal(app.store.getProposal(proposal.id).sourceSnapshot, 'cc'.repeat(32));
  });
});

test('review comment requires an existing proposal', async () => {
  await withServer(async (baseUrl) => {
    const response = await postJson(`${baseUrl}/admin/review-comments`, {
      proposalId: 'prop_missing',
      body: 'orphan',
      authorPrincipal: { type: 'user', id: 'local' },
    });
    assert.equal(response.status, 404);
  });
});

test('lane-submit creates open proposal and reuses same tip', async () => {
  await withServer(async (baseUrl) => {
    const project = (
      await postJson(`${baseUrl}/projects`, {
        organizationId: 'org_collab',
        name: 'Submit Project',
      }).then((r) => r.json())
    ).data;

    const payload = {
      projectId: project.id,
      syncRepoId: 'repo_lane_1',
      title: 'Submit feature',
      sourceLane: 'lane_feature',
      targetLane: 'lane_main',
      sourceSnapshot: 'bb'.repeat(32),
      authorPrincipal: { type: 'user', id: 'local' },
    };

    const first = await postJson(`${baseUrl}/collaboration/lane-submit`, payload);
    const firstBody = await first.json();
    assert.equal(first.status, 201);
    assert.equal(firstBody.reused, false);
    assert.equal(firstBody.data.status, 'open');
    assert.equal(firstBody.data.sourceLane, 'lane_feature');
    assert.match(first.headers.get('location'), /^\/admin\/proposals\/prop_/);

    const second = await postJson(`${baseUrl}/collaboration/lane-submit`, payload);
    const secondBody = await second.json();
    assert.equal(second.status, 200);
    assert.equal(secondBody.reused, true);
    assert.equal(secondBody.data.id, firstBody.data.id);

    const summary = await fetch(
      `${baseUrl}/collaboration/proposal-summary?projectId=${project.id}`,
    ).then((r) => r.json());
    assert.equal(summary.data.total, 1);
    assert.equal(summary.data.byStatus.open, 1);
  });
});

test('lane-submit attributes an authenticated session instead of a spoofed body principal', async () => {
  const sessionPrincipal = { type: 'user', id: 'oidc:verified-user' };
  const authAdapter = {
    mode: 'oidc',
    async resolveSession() {
      return {
        principal: sessionPrincipal,
        sessionId: 'test-session',
        authMode: 'oidc',
      };
    },
    mapPrincipal(identity) {
      return { type: 'user', id: identity.subject };
    },
  };

  await withServer(async (baseUrl) => {
    const project = (
      await postJson(`${baseUrl}/projects`, {
        organizationId: 'org_auth',
        name: 'Authenticated Submit',
      }).then((response) => response.json())
    ).data;

    const response = await postJson(`${baseUrl}/collaboration/lane-submit`, {
      projectId: project.id,
      syncRepoId: 'repo_auth',
      title: 'Authenticated feature',
      sourceLane: 'lane_auth',
      sourceSnapshot: 'cc'.repeat(32),
      authorPrincipal: { type: 'user', id: 'spoofed' },
    });
    const proposal = (await response.json()).data;

    assert.equal(response.status, 201);
    assert.deepEqual(proposal.authorPrincipal, sessionPrincipal);
  }, { authAdapter });
});

test('workflow run status updates', async () => {
  await withServer(async (baseUrl) => {
    const runRes = await postJson(`${baseUrl}/admin/workflow-runs`, {
      projectId: 'proj_ci',
      name: 'validate',
      requestedByPrincipal: { type: 'user', id: 'local' },
    });
    const run = (await runRes.json()).data;
    assert.equal(run.status, 'queued');

    const started = await patchJson(`${baseUrl}/admin/workflow-runs/${run.id}`, {
      status: 'in_progress',
    });
    const startedBody = await started.json();
    assert.equal(startedBody.data.status, 'in_progress');
    assert.ok(startedBody.data.startedAt);

    const done = await patchJson(`${baseUrl}/admin/workflow-runs/${run.id}`, {
      status: 'succeeded',
    });
    const doneBody = await done.json();
    assert.equal(doneBody.data.status, 'succeeded');
    assert.ok(doneBody.data.completedAt);

    const got = await fetch(`${baseUrl}/admin/workflow-runs/${run.id}`).then((r) => r.json());
    assert.equal(got.data.id, run.id);
  });
});

test('GET project by id', async () => {
  await withServer(async (baseUrl) => {
    const created = (
      await postJson(`${baseUrl}/projects`, {
        organizationId: 'org_x',
        name: 'By Id',
      }).then((r) => r.json())
    ).data;

    const response = await fetch(`${baseUrl}/projects/${created.id}`);
    const body = await response.json();
    assert.equal(response.status, 200);
    assert.equal(body.data.id, created.id);

    const missing = await fetch(`${baseUrl}/projects/proj_nope`);
    assert.equal(missing.status, 404);
  });
});

test('list proposals filters by status and sourceLane', async () => {
  await withServer(async (baseUrl) => {
    await postJson(`${baseUrl}/admin/proposals`, {
      projectId: 'proj_f',
      title: 'A',
      authorPrincipal: { type: 'user', id: 'local' },
      sourceLane: 'lane_a',
      status: 'open',
    });
    await postJson(`${baseUrl}/admin/proposals`, {
      projectId: 'proj_f',
      title: 'B',
      authorPrincipal: { type: 'user', id: 'local' },
      sourceLane: 'lane_b',
      status: 'draft',
    });

    const open = await fetch(`${baseUrl}/admin/proposals?status=open`).then((r) => r.json());
    assert.equal(open.data.length, 1);
    assert.equal(open.data[0].title, 'A');

    const laneB = await fetch(`${baseUrl}/admin/proposals?sourceLane=lane_b`).then((r) =>
      r.json(),
    );
    assert.equal(laneB.data.length, 1);
    assert.equal(laneB.data[0].title, 'B');
  });
});

test('admin and lane submissions share verified attribution and mirroring', async () => {
  const principal = { type: 'user', id: 'verified' };
  const mirrored = [];
  await withServer(async (baseUrl) => {
    const payload = {
      projectId: 'proj_one', title: 'Review', syncRepoId: 'repo_one', sourceLane: 'lane_feature',
      sourceSnapshot: 'aa'.repeat(32), authorPrincipal: { type: 'user', id: 'forged' },
      authorRef: 'user:also-forged',
    };
    const admin = await postJson(`${baseUrl}/admin/proposals`, payload).then((r) => r.json());
    const lane = await postJson(`${baseUrl}/collaboration/lane-submit`, { ...payload, projectId: 'proj_two' }).then((r) => r.json());
    for (const proposal of [admin.data, lane.data]) {
      assert.deepEqual(proposal.authorPrincipal, principal);
      assert.equal(proposal.authorRef, 'user:verified');
    }
    assert.notEqual(admin.data.id, lane.data.id, 'idempotency is scoped to the project');
    await patchJson(`${baseUrl}/admin/proposals/${admin.data.id}`, { status: 'open' });
    assert.deepEqual(mirrored.map((p) => p.id), [admin.data.id, lane.data.id, admin.data.id]);
    const comment = await postJson(`${baseUrl}/admin/review-comments`, {
      proposalId: admin.data.id, body: 'Verified comment', authorRef: 'user:forged',
    }).then((r) => r.json());
    assert.equal(comment.data.authorRef, 'user:verified');
  }, {
    authAdapter: { mode: 'oidc', async resolveSession() { return { principal, sessionId: 'verified-session', authMode: 'oidc' }; } },
    convexMirror: { async upsertProposal(proposal) { mirrored.push(proposal); } },
  });
});


test('lane-submit reuse is scoped to project and normalized repository', async () => {
  await withServer(async (baseUrl) => {
    const payload = {
      projectId: 'proj_a', syncRepoId: 'repo_shared', title: 'Tip',
      sourceLane: 'lane_feature', sourceSnapshot: 'dd'.repeat(32),
    };
    const first = await postJson(`${baseUrl}/collaboration/lane-submit`, payload);
    const initial = (await first.json()).data;
    assert.equal(first.status, 201);

    const otherProject = await postJson(`${baseUrl}/collaboration/lane-submit`, {
      ...payload, projectId: 'proj_b',
    });
    assert.equal(otherProject.status, 201);
    assert.equal((await otherProject.json()).data.projectId, 'proj_b');

    const repeated = await postJson(`${baseUrl}/collaboration/lane-submit`, {
      ...payload, projectId: ' proj_a ', syncRepoId: ' repo_shared ',
    });
    assert.equal(repeated.status, 200);
    assert.equal((await repeated.json()).data.id, initial.id);

    const withoutRepository = await postJson(`${baseUrl}/collaboration/lane-submit`, {
      ...payload, syncRepoId: undefined,
    });
    assert.equal(withoutRepository.status, 201);
    const noRepo = (await withoutRepository.json()).data;
    assert.notEqual(noRepo.id, initial.id);
    const repeatedNoRepo = await postJson(`${baseUrl}/collaboration/lane-submit`, {
      ...payload, syncRepoId: undefined,
    });
    assert.equal(repeatedNoRepo.status, 200);
    assert.equal((await repeatedNoRepo.json()).data.id, noRepo.id);
  });
});
