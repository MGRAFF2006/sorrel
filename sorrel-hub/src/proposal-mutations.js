import { bindSessionPrincipal } from './policy-guard.js';

/** All proposal writes share attribution and best-effort metadata mirroring. */
export function createProposal(attributes, context) {
  const proposal = context.store.createProposal(bindSessionPrincipal(attributes, context));
  mirrorProposal(proposal, context);
  return proposal;
}

export function updateProposal(id, attributes, context) {
  const proposal = context.store.updateProposal(id, attributes);
  mirrorProposal(proposal, context);
  return proposal;
}

function mirrorProposal(proposal, context) {
  if (!context.convexMirror) return;
  void Promise.resolve().then(() => context.convexMirror.upsertProposal(proposal)).catch((error) => {
    console.warn(`[convex-mirror] proposal ${proposal.id}: ${error instanceof Error ? error.message : String(error)}`);
  });
}
