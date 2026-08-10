const DEFAULT_POLICY = {
  mask: "***",
  minSecretLength: 6,
  visiblePrefix: 0,
  visibleSuffix: 0,
  detectEnvKeys: ["TOKEN", "SECRET", "PASSWORD", "KEY"]
};

export function redactValue(value, policy = DEFAULT_POLICY) {
  const text = String(value);
  const prefixLength = Math.min(policy.visiblePrefix ?? 0, text.length);
  const suffixLength = Math.min(policy.visibleSuffix ?? 0, Math.max(text.length - prefixLength, 0));
  const prefix = text.slice(0, prefixLength);
  const suffix = suffixLength > 0 ? text.slice(-suffixLength) : "";

  return `${prefix}${policy.mask ?? DEFAULT_POLICY.mask}${suffix}`;
}

export function redactText(text, secretValues, policy = DEFAULT_POLICY, { secretRefs = [] } = {}) {
  let redacted = String(text);
  const mergedPolicy = { ...DEFAULT_POLICY, ...policy };

  for (const value of uniqueSecretValues(secretValues)) {
    const secret = String(value);

    if (secret.length < mergedPolicy.minSecretLength) {
      continue;
    }

    redacted = redacted.replaceAll(secret, redactValue(secret, mergedPolicy));
  }

  return redactEnvAssignments(redactSecretReferences(redacted, secretRefs, mergedPolicy), mergedPolicy);
}

export function redactSecretReferences(text, secretRefs, policy = DEFAULT_POLICY) {
  let redacted = String(text);
  const mergedPolicy = { ...DEFAULT_POLICY, ...policy };

  for (const value of uniqueSecretValues(secretRefs)) {
    const secretRef = String(value);

    if (secretRef.length === 0) {
      continue;
    }

    redacted = redacted.replaceAll(secretRef, redactValue(secretRef, mergedPolicy));
  }

  return redacted;
}

export function redactEnvAssignments(text, policy = DEFAULT_POLICY) {
  const mergedPolicy = { ...DEFAULT_POLICY, ...policy };
  const detectors = mergedPolicy.detectEnvKeys ?? DEFAULT_POLICY.detectEnvKeys;

  return String(text)
    .split(/\r?\n/)
    .map((line) => {
      const match = line.match(/^([A-Za-z_][A-Za-z0-9_]*)(=)(.*)$/);

      if (!match) {
        return line;
      }

      const [, key, separator, value] = match;

      if (!detectors.some((detector) => key.includes(detector))) {
        return line;
      }

      return `${key}${separator}${redactValue(value, mergedPolicy)}`;
    })
    .join("\n");
}

export function collectResolvedSecretValues(resolvedSecrets) {
  return resolvedSecrets
    .map((resolved) => resolved.value)
    .filter((value) => typeof value === "string" && value.length > 0);
}

export function collectResolvedSecretRefs(resolvedSecrets) {
  return resolvedSecrets.flatMap((resolved) =>
    [
      resolved.secretRef?.id,
      resolved.secretRef?.uri,
      resolved.storeKey,
      resolved.redaction?.resource?.id,
      resolved.redaction?.resource?.uri
    ].filter((value) => typeof value === "string" && value.length > 0)
  );
}

export function redactionMetadata(policy = DEFAULT_POLICY) {
  const mergedPolicy = { ...DEFAULT_POLICY, ...policy };

  return {
    schemaVersion: "sorrel.protocol.v0",
    kind: "RedactionMetadata",
    strategy: "mask",
    mask: mergedPolicy.mask,
    minSecretLength: mergedPolicy.minSecretLength,
    visiblePrefix: mergedPolicy.visiblePrefix,
    visibleSuffix: mergedPolicy.visibleSuffix,
    detectEnvKeys: mergedPolicy.detectEnvKeys
  };
}

function uniqueSecretValues(secretValues) {
  const values = Array.isArray(secretValues) ? secretValues : Array.from(secretValues ?? []);
  return [...new Set(values)].sort((left, right) => String(right).length - String(left).length);
}
