export const DEFAULT_LIMITS = Object.freeze({
  requestBodyBytes: 64 * 1024 * 1024,
  traversalLinks: 100_000,
  traversalObjects: 100_000,
  traversalBytes: 256 * 1024 * 1024,
});

const ENV_KEYS = {
  requestBodyBytes: 'SORREL_HUB_MAX_BODY_BYTES',
  traversalLinks: 'SORREL_HUB_MAX_TRAVERSAL_LINKS',
  traversalObjects: 'SORREL_HUB_MAX_TRAVERSAL_OBJECTS',
  traversalBytes: 'SORREL_HUB_MAX_TRAVERSAL_BYTES',
};

export function resolveResourceLimits(env = process.env) {
  const limits = { ...DEFAULT_LIMITS };
  for (const [key, variable] of Object.entries(ENV_KEYS)) {
    const raw = env[variable];
    if (raw === undefined) continue;
    if (!/^[1-9][0-9]*$/.test(raw) || !Number.isSafeInteger(Number(raw))) {
      throw new Error(`${variable} must be a positive safe integer`);
    }
    limits[key] = Number(raw);
  }
  return limits;
}

