import path from "node:path";
import { loadDotEnvFile } from "./dotenv.mjs";
import {
  AccessDeniedError,
  POLICY_DECISION_STATUS,
  coreSecretCapabilityForAction,
  evaluateCorePolicyFromGrants
} from "./grants.mjs";
import { LocalDevSecretBackend } from "./local-backend.mjs";
import {
  collectResolvedSecretRefs,
  collectResolvedSecretValues,
  redactText
} from "./redaction.mjs";
import { loadSecretSpec } from "./spec-loader.mjs";

export const DEFAULT_SPEC_PATH = path.join("examples", "sorrel.secrets.dev.yml");

export class CliError extends Error {
  constructor(message, { exitCode = 1, cause } = {}) {
    super(message);
    this.name = "CliError";
    this.exitCode = exitCode;
    if (cause !== undefined) {
      this.cause = cause;
    }
  }
}

/**
 * Resolve a spec path against a base directory and load it.
 * Returns the parsed spec plus the base directory used for env-file resolution.
 */
export async function loadSpecForCli({ specPath = DEFAULT_SPEC_PATH, baseDir = process.cwd() } = {}) {
  const resolvedSpecPath = path.resolve(baseDir, specPath);
  let spec;

  try {
    spec = await loadSecretSpec(resolvedSpecPath);
  } catch (error) {
    throw new CliError(`Failed to load secret spec at ${specPath}: ${error.message}`, {
      cause: error
    });
  }

  return { spec, specPath: resolvedSpecPath, baseDir };
}

function indexSecretRefs(spec) {
  return new Map((spec.secretRefs ?? []).map((secretRef) => [secretRef.id, secretRef]));
}

function bindingsByEnvironment(spec, secretId) {
  const result = new Map();
  for (const binding of spec.localDev?.bindings ?? []) {
    if (binding.secret?.id === secretId) {
      result.set(binding.environment, binding);
    }
  }
  return result;
}

/**
 * `list` command: enumerate declared SecretRef handles and the environments
 * that define or bind them. Raw values are never read or returned.
 */
export function listSecretRefs(spec, { environment } = {}) {
  const refs = (spec.secretRefs ?? []).map((secretRef) => {
    const bindings = bindingsByEnvironment(spec, secretRef.id);
    const grantEnvironments = new Set(
      (spec.grants ?? [])
        .filter((grant) => grant.secret?.id === secretRef.id)
        .map((grant) => grant.environment)
    );

    return {
      id: secretRef.id,
      name: secretRef.name,
      provider: secretRef.provider,
      uri: secretRef.uri,
      valueType: secretRef.valueType,
      required: Boolean(secretRef.required),
      environment: secretRef.environment,
      boundEnvironments: [...bindings.keys()].sort(),
      grantEnvironments: [...grantEnvironments].sort(),
      description: secretRef.description
    };
  });

  const filtered = environment
    ? refs.filter(
        (ref) =>
          ref.environment === environment ||
          ref.boundEnvironments.includes(environment) ||
          ref.grantEnvironments.includes(environment)
      )
    : refs;

  return { secretRefs: filtered, count: filtered.length };
}

/**
 * Build an actor tuple from a principal/actor reference.
 *
 * Grants in sorrel-vault may constrain agent, workflow, AND runner dimensions
 * simultaneously, so a single bare id rarely satisfies a fully-specified grant.
 * To support both simple and precise principals, --principal accepts one or more
 * comma-separated components:
 *
 *   - "Kind:id" pairs map to the matching actor slot, e.g.
 *     "AgentPolicy:agent_policy_local_dev,Workflow:workflow_validate_vault,Runner:runner_local_process"
 *   - a bare "id" is applied across all three slots so grants that constrain a
 *     single dimension still match.
 */
export function buildActor(principal) {
  if (!principal) {
    return {};
  }

  const components = principal
    .split(",")
    .map((part) => part.trim())
    .filter((part) => part.length > 0);

  const actor = {};

  for (const component of components) {
    const separatorIndex = component.indexOf(":");

    if (separatorIndex === -1) {
      // Bare id: apply across all slots so any constrained dimension can match.
      actor.agent = { kind: "AgentPolicy", id: component };
      actor.workflow = { kind: "Workflow", id: component };
      actor.runner = { kind: "Runner", id: component };
      continue;
    }

    const kind = component.slice(0, separatorIndex);
    const id = component.slice(separatorIndex + 1);
    const slot = actorSlotForKind(kind);

    if (!slot) {
      throw new CliError(
        `Unknown principal kind "${kind}"; expected one of AgentPolicy, Workflow, Runner`
      );
    }

    actor[slot] = { kind, id };
  }

  return actor;
}

function actorSlotForKind(kind) {
  switch (kind) {
    case "AgentPolicy":
      return "agent";
    case "Workflow":
      return "workflow";
    case "Runner":
      return "runner";
    default:
      return undefined;
  }
}

/**
 * `grant` command: evaluate access for a principal/secret/environment/action.
 * Composes evaluateCorePolicyFromGrants; returns a redaction-safe decision and
 * never reads or returns a secret value.
 */
export function evaluateGrant(spec, { secret, environment, action = "read", principal } = {}) {
  if (!secret) {
    throw new CliError("grant requires --secret <secretRefId>");
  }
  if (!environment) {
    throw new CliError("grant requires --env <environment>");
  }

  const secretRefs = indexSecretRefs(spec);
  if (!secretRefs.has(secret)) {
    throw new CliError(`Unknown SecretRef: ${secret}`);
  }

  const actor = buildActor(principal);
  const request = {
    secret: { kind: "SecretRef", id: secret },
    environment,
    action,
    actor
  };

  const decision = evaluateCorePolicyFromGrants(spec, request);

  return {
    secret,
    environment,
    action,
    capability: coreSecretCapabilityForAction(action),
    status: decision.status,
    allowed: decision.status === POLICY_DECISION_STATUS.ALLOW,
    grant: decision.grant?.id,
    reason: decision.reason
  };
}

/**
 * `import` command: import a .env file into the local backend for an
 * environment. Reports the keys that were bound to SecretRefs without ever
 * returning or printing raw values.
 */
export async function importEnv(
  spec,
  { baseDir = process.cwd(), environment, file } = {}
) {
  const backend = new LocalDevSecretBackend(spec, { baseDir });
  let importedKeys;

  if (file) {
    const resolvedFile = path.resolve(baseDir, file);
    let values;
    try {
      values = await loadDotEnvFile(resolvedFile);
    } catch (error) {
      throw new CliError(`Failed to read .env file ${file}: ${error.message}`, { cause: error });
    }

    const targetEnv = environment ?? "*";
    backend.importValues(values, targetEnv);
    importedKeys = boundKeysFor(spec, values, environment);
  } else {
    try {
      await backend.importEnvFiles();
    } catch (error) {
      throw new CliError(`Failed to import declared .env files: ${error.message}`, { cause: error });
    }
    importedKeys = boundKeysFromBackend(spec, backend, environment);
  }

  return {
    environment: environment ?? null,
    importedCount: importedKeys.length,
    importedKeys
  };
}

function boundKeysFor(spec, values, environment) {
  const lookup = values instanceof Map ? values : new Map(Object.entries(values));
  const keys = [];

  for (const binding of spec.localDev?.bindings ?? []) {
    if (environment && binding.environment !== environment) {
      continue;
    }
    if (lookup.has(binding.envKey)) {
      keys.push({
        secret: binding.secret?.id,
        envKey: binding.envKey,
        environment: binding.environment,
        storeKey: binding.storeKey
      });
    }
  }

  return keys;
}

function boundKeysFromBackend(spec, backend, environment) {
  const keys = [];
  for (const binding of spec.localDev?.bindings ?? []) {
    if (environment && binding.environment !== environment) {
      continue;
    }
    if (backend.valuesByStoreKey.has(binding.storeKey)) {
      keys.push({
        secret: binding.secret?.id,
        envKey: binding.envKey,
        environment: binding.environment,
        storeKey: binding.storeKey
      });
    }
  }
  return keys;
}

/**
 * `redact` command: redact text using the spec's resolved secret values/refs and
 * the spec's redaction policy. Mirrors scripts/local-dev-backend.mjs by
 * importing values, resolving granted handles, and feeding redactText.
 *
 * Resolution requires a Core policy; for the CLI we wire the local-dev adapter
 * (createLocalDevCorePolicy semantics via evaluateCorePolicyFromGrants) so that
 * only granted handles contribute their values to the redaction set. Handles
 * that are not granted are skipped silently (their values are never emitted).
 */
export async function redactInput(
  spec,
  text,
  { baseDir = process.cwd(), environment, principal } = {}
) {
  const backend = new LocalDevSecretBackend(spec, {
    baseDir,
    corePolicy: {
      evaluate(policyRequest) {
        return evaluateCorePolicyFromGrants(spec, {
          secret: policyRequest.resource,
          environment: policyRequest.environment,
          action: policyRequest.action,
          actor: policyRequest.subject ?? {}
        });
      }
    }
  });

  await backend.importEnvFiles();

  const actor = buildActor(principal);
  const resolved = [];

  for (const secretRef of spec.secretRefs ?? []) {
    if (environment && secretRef.environment !== environment) {
      continue;
    }

    try {
      resolved.push(
        backend.resolve({
          secret: { kind: "SecretRef", id: secretRef.id },
          environment: secretRef.environment,
          action: "redact",
          actor
        })
      );
    } catch (error) {
      if (error instanceof AccessDeniedError) {
        // Not granted for this actor; skip. Its value is never surfaced.
        continue;
      }
      throw error;
    }
  }

  const redacted = redactText(text, collectResolvedSecretValues(resolved), spec.redaction, {
    secretRefs: collectResolvedSecretRefs(resolved)
  });

  return {
    redacted,
    redactedSecretCount: resolved.length
  };
}

/**
 * Parse a value of text from either inline text, a file, or stdin.
 */
export async function readTextInput({ text, file, stdin } = {}) {
  if (typeof text === "string") {
    return text;
  }

  if (file) {
    const { readFile } = await import("node:fs/promises");
    try {
      return await readFile(file, "utf8");
    } catch (error) {
      throw new CliError(`Failed to read input file ${file}: ${error.message}`, { cause: error });
    }
  }

  if (stdin) {
    return readStream(stdin);
  }

  throw new CliError("redact requires --file <path> or piped stdin");
}

function readStream(stream) {
  return new Promise((resolve, reject) => {
    let data = "";
    stream.setEncoding("utf8");
    stream.on("data", (chunk) => {
      data += chunk;
    });
    stream.on("end", () => resolve(data));
    stream.on("error", reject);
  });
}

/**
 * Minimal argv parser: collects `--flag value` and `--flag=value` pairs and the
 * first positional as the command. Boolean flags (e.g. --help) are supported via
 * the booleanFlags set.
 */
export function parseArgs(argv, { booleanFlags = new Set(["help"]) } = {}) {
  const flags = {};
  const positionals = [];

  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];

    if (token.startsWith("--")) {
      const body = token.slice(2);
      const eqIndex = body.indexOf("=");

      if (eqIndex !== -1) {
        flags[body.slice(0, eqIndex)] = body.slice(eqIndex + 1);
        continue;
      }

      if (booleanFlags.has(body)) {
        flags[body] = true;
        continue;
      }

      const next = argv[i + 1];
      if (next === undefined || next.startsWith("--")) {
        flags[body] = true;
      } else {
        flags[body] = next;
        i += 1;
      }
      continue;
    }

    positionals.push(token);
  }

  return { command: positionals[0], positionals: positionals.slice(1), flags };
}
