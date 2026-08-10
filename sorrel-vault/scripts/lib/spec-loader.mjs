import { readFile } from "node:fs/promises";
import path from "node:path";
import * as yaml from "js-yaml";

export async function loadSecretSpec(filePath) {
  const source = await readFile(filePath, "utf8");
  const spec = yaml.load(source, {
    filename: filePath,
    schema: yaml.CORE_SCHEMA
  });

  if (!spec || typeof spec !== "object" || Array.isArray(spec)) {
    throw new Error(`${filePath}: expected a YAML mapping at the document root`);
  }

  return spec;
}

export function specBaseDir(root, specPath) {
  return path.isAbsolute(specPath) ? path.dirname(specPath) : root;
}

export function validateSecretSpecSemantics(spec, { filePath = "<spec>" } = {}) {
  const errors = [];
  const environments = new Set(Object.keys(spec.environments ?? {}));
  const secretRefs = new Map();

  for (const secretRef of spec.secretRefs ?? []) {
    if (secretRefs.has(secretRef.id)) {
      errors.push(`${filePath}: duplicate SecretRef id ${secretRef.id}`);
    }

    secretRefs.set(secretRef.id, secretRef);

    if (!environments.has(secretRef.environment)) {
      errors.push(`${filePath}: SecretRef ${secretRef.id} references unknown environment ${secretRef.environment}`);
    }
  }

  for (const grant of spec.grants ?? []) {
    if (!secretRefs.has(grant.secret?.id)) {
      errors.push(`${filePath}: grant ${grant.id} references unknown SecretRef ${grant.secret?.id}`);
    }

    if (!environments.has(grant.environment)) {
      errors.push(`${filePath}: grant ${grant.id} references unknown environment ${grant.environment}`);
    }
  }

  for (const binding of spec.localDev?.bindings ?? []) {
    const secretRef = secretRefs.get(binding.secret?.id);

    if (!secretRef) {
      errors.push(`${filePath}: local binding references unknown SecretRef ${binding.secret?.id}`);
      continue;
    }

    if (secretRef.environment !== binding.environment) {
      errors.push(
        `${filePath}: local binding for ${binding.secret.id} uses ${binding.environment}, expected ${secretRef.environment}`
      );
    }
  }

  for (const violation of findRawValueFields(spec)) {
    errors.push(`${filePath}: raw secret value field is not allowed at ${violation}`);
  }

  return errors;
}

function findRawValueFields(node, pathParts = []) {
  if (!node || typeof node !== "object") {
    return [];
  }

  const violations = [];

  for (const [key, value] of Object.entries(node)) {
    const nextPath = [...pathParts, key];

    if (["value", "secretValue", "plaintext", "plainText"].includes(key)) {
      violations.push(nextPath.join("."));
    }

    violations.push(...findRawValueFields(value, nextPath));
  }

  return violations;
}
