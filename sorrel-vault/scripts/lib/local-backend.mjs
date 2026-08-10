import { constants } from "node:fs";
import { access } from "node:fs/promises";
import path from "node:path";
import { AccessDeniedError, assertCorePolicyAllowed } from "./grants.mjs";
import { loadDotEnvFile } from "./dotenv.mjs";
import { redactValue, redactionMetadata } from "./redaction.mjs";

export class LocalDevSecretBackend {
  constructor(spec, { baseDir = process.cwd(), corePolicy, auditSink } = {}) {
    if (spec.localDev?.backend !== "local-dev") {
      throw new Error("LocalDevSecretBackend only supports localDev.backend: local-dev");
    }

    this.spec = spec;
    this.baseDir = baseDir;
    this.corePolicy = corePolicy;
    this.auditSink = auditSink;
    this.auditEvents = [];
    this.valuesByStoreKey = new Map();
    this.bindingsBySecret = indexBindings(spec.localDev.bindings);
    this.secretRefsById = new Map(spec.secretRefs.map((secretRef) => [secretRef.id, secretRef]));
  }

  async importEnvFiles() {
    for (const envFile of this.spec.localDev.import.envFiles) {
      const filePath = path.resolve(this.baseDir, envFile.path);

      if (!(await fileExists(filePath))) {
        if (envFile.required) {
          throw new Error(`Required .env file not found: ${envFile.path}`);
        }

        continue;
      }

      const values = await loadDotEnvFile(filePath);
      this.importValues(values, envFile.environment);
    }

    if (this.spec.localDev.import.allowProcessEnvFallback) {
      this.importValues(process.env, "*");
    }
  }

  importValues(values, environment) {
    const lookup = values instanceof Map ? values : new Map(Object.entries(values));

    for (const binding of this.spec.localDev.bindings) {
      if (environment !== "*" && binding.environment !== environment) {
        continue;
      }

      if (!lookup.has(binding.envKey)) {
        continue;
      }

      this.valuesByStoreKey.set(binding.storeKey, String(lookup.get(binding.envKey)));
    }
  }

  resolve({ secret, environment, action = "read", actor = {}, corePolicy } = {}) {
    const secretRef = this.getSecretRef(secret);
    const binding = this.getBinding(secretRef.id, environment);
    const request = {
      secret: { kind: "SecretRef", id: secretRef.id },
      environment,
      action,
      actor
    };
    let policy;

    try {
      policy = assertCorePolicyAllowed(this.spec, request, corePolicy ?? this.corePolicy);
    } catch (error) {
      if (error instanceof AccessDeniedError) {
        error.auditEvent = this.emitAuditEvent(
          secretAuditEvent({
            request,
            secretRef,
            decision: error.decision,
            redaction: redactionDetails(secretRef, this.spec.redaction)
          })
        );
      }

      throw error;
    }

    const value = this.valuesByStoreKey.get(binding.storeKey);

    if (value === undefined && secretRef.required) {
      throw new Error(`Required secret ${secretRef.id} has no imported local value`);
    }

    const redaction = redactionDetails(secretRef, this.spec.redaction);
    const auditEvent = this.emitAuditEvent(
      secretAuditEvent({
        request,
        secretRef,
        decision: policy.decision,
        grant: policy.grant,
        redaction
      })
    );

    return {
      secretRef,
      grant: policy.grant,
      policyDecision: policy.decision,
      storeKey: binding.storeKey,
      value,
      redacted: value === undefined ? undefined : redactValue(value, this.spec.redaction),
      redaction,
      auditEvent
    };
  }

  materializeEnv(requests) {
    const env = {};

    for (const request of requests) {
      const resolved = this.resolve({ ...request, action: request.action ?? "materialize" });
      env[resolved.secretRef.name] = resolved.value ?? "";
    }

    return env;
  }

  getAuditEvents() {
    return [...this.auditEvents];
  }

  emitAuditEvent(event) {
    this.auditEvents.push(event);

    if (this.auditSink) {
      this.auditSink(event);
    }

    return event;
  }

  getSecretRef(secret) {
    const id = typeof secret === "string" ? secret : secret?.id;
    const secretRef = this.secretRefsById.get(id);

    if (!secretRef) {
      throw new Error(`Unknown SecretRef: ${id}`);
    }

    return secretRef;
  }

  getBinding(secretId, environment) {
    const binding = this.bindingsBySecret.get(`${secretId}:${environment}`);

    if (!binding) {
      throw new Error(`No local binding for ${secretId} in ${environment}`);
    }

    return binding;
  }
}

function indexBindings(bindings) {
  return new Map(bindings.map((binding) => [`${binding.secret.id}:${binding.environment}`, binding]));
}

function secretAuditEvent({ request, secretRef, decision, grant, redaction }) {
  return {
    schemaVersion: "sorrel.protocol.v0",
    kind: "AuditEvent",
    type: "secret.access",
    time: new Date().toISOString(),
    capability: decision.capability,
    action: request.action,
    outcome: decision.status,
    subject: request.actor ?? {},
    resource: {
      kind: "SecretRef",
      id: secretRef.id,
      name: secretRef.name,
      uri: secretRef.uri
    },
    environment: request.environment,
    grant: decision.grant ?? (grant ? { kind: "SecretGrant", id: grant.id } : undefined),
    policy: decision.policy ?? grant?.policy,
    reason: decision.reason,
    redaction
  };
}

function redactionDetails(secretRef, policy) {
  return {
    ...redactionMetadata(policy),
    resource: {
      kind: "SecretRef",
      id: secretRef.id,
      name: secretRef.name,
      uri: secretRef.uri
    },
    redacts: ["value", "uri", "storeKey"]
  };
}

async function fileExists(filePath) {
  try {
    await access(filePath, constants.R_OK);
    return true;
  } catch {
    return false;
  }
}
