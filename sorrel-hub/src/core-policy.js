import { spawn } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const POLICY_ACTION_GRANT = 'policy.grant';
const MAX_BYTES = 1024 * 1024;
const MAX_CONCURRENT = 16;
const TIMEOUT_MS = 5000;
let active = 0;

export class PolicyEvaluationError extends Error {
  constructor(message, statusCode) {
    super(message);
    this.name = 'PolicyEvaluationError';
    this.code = 'policy_evaluation_failed';
    this.statusCode = statusCode;
  }
}

export class PolicyDeniedError extends Error {
  constructor(message, decision) {
    super(message);
    this.name = 'PolicyDeniedError';
    this.code = 'policy_denied';
    this.decision = decision;
  }
}

function isPlainObject(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

/** Validate references without letting caller-selected IDs remove effective denies. */
export function hydrateTrustedGrants(grantRefs, trustedGrantsById = {}) {
  if (!Array.isArray(grantRefs)) throw new PolicyEvaluationError('grantRefs must be an array');
  return grantRefs.map((ref) => {
    if (!isPlainObject(ref) || typeof ref.id !== 'string' || !ref.id.trim()) {
      throw new PolicyEvaluationError('each grant reference must contain a non-empty string id');
    }
    if (!Object.hasOwn(trustedGrantsById, ref.id)) {
      throw new PolicyEvaluationError('referenced grant is not available for Core evaluation');
    }
    return trustedGrantsById[ref.id];
  });
}

function trustedRecords(records) {
  if (!isPlainObject(records)) throw new PolicyEvaluationError('trusted authorization records must be an object');
  return Object.entries(records).map(([id, record]) => {
    if (!isPlainObject(record) || record.id !== id) {
      throw new PolicyEvaluationError('trusted authorization record ID must match its configured key');
    }
    return record;
  });
}

function policyReferences(context) {
  const references = context.policyRefs ?? [];
  if (!Array.isArray(references)) throw new PolicyEvaluationError('policyRefs must be an array');
  const all = context.policyRef ? [...references, context.policyRef] : references;
  for (const ref of all) {
    if (!isPlainObject(ref) || ref.kind !== 'Policy' || typeof ref.id !== 'string' ||
        !Object.hasOwn(context.trustedPoliciesById ?? {}, ref.id)) {
      throw new PolicyEvaluationError('referenced policy is not available for Core evaluation');
    }
  }
}

/** Transport only: authorization semantics and decisions are owned by Rust Core. */
export async function evaluate(request) {
  const payload = Buffer.from(JSON.stringify({
    principal: request.principal,
    action: request.action,
    resource: request.resource,
    grants: request.grants ?? [],
    policies: request.policies ?? [],
  }));
  if (payload.length > MAX_BYTES) throw new PolicyEvaluationError('Core policy request exceeds byte limit');
  if (active >= MAX_CONCURRENT) throw new PolicyEvaluationError('Core policy evaluator is busy', 503);
  const root = fileURLToPath(new URL('../../', import.meta.url));
  const target = process.env.CARGO_TARGET_DIR
    ? path.resolve(process.env.CARGO_TARGET_DIR) : path.join(root, 'target');
  const binary = process.env.SORREL_HUB_CORE_POLICY_BIN ?? path.join(
    target, 'debug',
    `sorrel-core-policy${process.platform === 'win32' ? '.exe' : ''}`,
  );
  active += 1;
  return await new Promise((resolve, reject) => {
    let child;
    try { child = spawn(binary, [], { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true }); }
    catch {
      active -= 1;
      return reject(new PolicyEvaluationError('Core policy executable could not be started', 503));
    }
    const output = [];
    let bytes = 0;
    let failure;
    const fail = (message) => {
      failure ??= new PolicyEvaluationError(message, 503);
      child.kill('SIGKILL');
    };
    const timer = setTimeout(() => fail('Core policy evaluator timed out'), TIMEOUT_MS);
    child.on('error', () => { failure ??= new PolicyEvaluationError('Core policy executable is unavailable; build or configure SORREL_HUB_CORE_POLICY_BIN', 503); });
    child.stdin.on('error', () => fail('Core policy request could not be delivered'));
    for (const stream of [child.stdout, child.stderr]) {
      stream.on('data', (chunk) => {
        bytes += chunk.length;
        if (bytes > MAX_BYTES) return fail('Core policy response exceeds byte limit');
        if (stream === child.stdout) output.push(chunk);
      });
    }
    child.on('close', (code) => {
      clearTimeout(timer);
      active -= 1;
      if (failure) return reject(failure);
      let result;
      try { result = JSON.parse(Buffer.concat(output).toString('utf8')); }
      catch { return reject(new PolicyEvaluationError('Core policy evaluator returned an invalid response', 503)); }
      if (!isPlainObject(result)) return reject(new PolicyEvaluationError('Core policy evaluator returned an invalid response', 503));
      if (result.error) return reject(new PolicyEvaluationError(result.error.message ?? 'Core policy evaluation failed'));
      if (code !== 0 || result.decision?.schemaVersion !== 'sorrel.protocol.v0' ||
          result.decision?.kind !== 'PolicyDecision' || typeof result.allowed !== 'boolean' ||
          result.allowed !== (result.decision.decision === 'allow')) {
        return reject(new PolicyEvaluationError('Core policy evaluator returned an invalid decision', 503));
      }
      resolve(result);
    });
    child.stdin.end(payload);
  });
}

export async function evaluateWithTrustedGrants(
  principal, action, resource, grantRefs, trustedGrantsById = {}, policyContext = {},
) {
  hydrateTrustedGrants(grantRefs, trustedGrantsById);
  policyReferences(policyContext);
  const result = await evaluate({
    principal, action, resource,
    grants: trustedRecords(trustedGrantsById),
    policies: trustedRecords(policyContext.trustedPoliciesById ?? {}),
  });
  if (!result.allowed) throw new PolicyDeniedError('Core policy denied request', result.decision);
  return result;
}
