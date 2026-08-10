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
