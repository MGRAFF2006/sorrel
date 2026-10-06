/**
 * Optional server-only proposal metadata mirror. VCS objects/refs stay in Hub.
 * Requires a deployment URL and privileged admin key; failures remain best effort.
 */

export function isConvexMirrorEnabled(env = process.env) {
  return env.SORREL_HUB_CONVEX !== '0' && env.SORREL_HUB_CONVEX !== 'false' &&
    Boolean(env.CONVEX_URL || env.CONVEX_SELF_HOSTED_URL) &&
    Boolean(env.CONVEX_DEPLOY_KEY || env.CONVEX_SELF_HOSTED_ADMIN_KEY);
}

/**
 * @param {NodeJS.ProcessEnv} [env]
 */
export function createConvexMirror(env = process.env) {
  const url = env.CONVEX_URL || env.CONVEX_SELF_HOSTED_URL || '';
  const adminKey = env.CONVEX_DEPLOY_KEY || env.CONVEX_SELF_HOSTED_ADMIN_KEY || '';

  if (!isConvexMirrorEnabled(env)) {
    return {
      enabled: false,
      async upsertProposal() {},
      async removeProposal() {},
    };
  }

  /**
   * Best-effort HTTP mutation against self-hosted / cloud Convex.
   * The admin header permits calls to internal functions.
   *
   * @param {string} path
   * @param {unknown} body
   */
  async function postMutation(path, body) {
    try {
      const headers = {
        'content-type': 'application/json',
        accept: 'application/json',
      };
      headers.authorization = `Convex ${adminKey}`;
      const response = await fetch(`${url.replace(/\/$/, '')}${path}`, {
        method: 'POST',
        headers,
        body: JSON.stringify(body),
      });
      if (!response.ok) {
        await response.body?.cancel();
        console.warn(`[convex-mirror] ${path} failed: HTTP ${response.status}`);
      }
    } catch (error) {
      console.warn(`[convex-mirror] ${path} request failed`);
    }
  }

  return {
    enabled: true,
    /**
     * @param {{ id: string, status?: string, projectId?: string, title?: string, updatedAt?: string }} proposal
     */
    async upsertProposal(proposal) {
      await postMutation('/api/mutation', {
        path: 'proposals:upsert',
        args: {
          hubId: proposal.id,
          status: proposal.status ?? 'open',
          projectId: proposal.projectId,
          title: proposal.title,
          updatedAt: proposal.updatedAt ?? new Date().toISOString(),
        },
        format: 'json',
      });
    },
    /**
     * @param {string} hubId
     */
    async removeProposal(hubId) {
      await postMutation('/api/mutation', {
        path: 'proposals:remove',
        args: { hubId },
        format: 'json',
      });
    },
  };
}
