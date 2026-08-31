/**
 * Optional Convex metadata mirror for the live open-proposals spike.
 *
 * VCS objects/refs stay on the sync object store. This client only mirrors
 * proposal status so the Solid UI can subscribe to `proposals.countOpen`.
 *
 * Enabled when CONVEX_URL is set. Failures are logged and swallowed so Hub
 * remains available without Convex.
 */

/**
 * @param {NodeJS.ProcessEnv} [env]
 */
export function createConvexMirror(env = process.env) {
  const url = env.CONVEX_URL || env.CONVEX_SELF_HOSTED_URL || '';
  const adminKey = env.CONVEX_DEPLOY_KEY || env.CONVEX_SELF_HOSTED_ADMIN_KEY || '';

  if (!url) {
    return {
      enabled: false,
      async upsertProposal() {},
      async removeProposal() {},
    };
  }

  /**
   * Best-effort HTTP mutation against self-hosted / cloud Convex.
   * Full typed client lands with Phase 3 metadata migration.
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
      if (adminKey) {
        headers.authorization = `Convex ${adminKey}`;
      }
      const response = await fetch(`${url.replace(/\/$/, '')}${path}`, {
        method: 'POST',
        headers,
        body: JSON.stringify(body),
      });
      if (!response.ok) {
        const text = await response.text();
        console.warn(`[convex-mirror] ${path} failed: ${response.status} ${text.slice(0, 200)}`);
      }
    } catch (error) {
      console.warn(`[convex-mirror] ${path} error:`, error instanceof Error ? error.message : error);
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
