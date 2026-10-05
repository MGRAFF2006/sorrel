import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { parseDotEnv } from "../scripts/lib/dotenv.mjs";
import { LocalDevSecretBackend } from "../scripts/lib/local-backend.mjs";
import { redactText, redactSecretReferences } from "../scripts/lib/redaction.mjs";

test("redaction treats replacement syntax in masks literally", () => {
  const policy = { mask: "$&$`$'$$", minSecretLength: 1 };
  assert.equal(redactText("before fixture-secret after", ["fixture-secret"], policy), "before $&$`$'$$ after");
  assert.equal(redactSecretReferences("before fixture-ref after", ["fixture-ref"], policy), "before $&$`$'$$ after");
});

test("dotenv parses quoted values before trailing comments and rejects malformed quotes", () => {
  const values = parseDotEnv([
    'DOUBLE="fixture # value" # comment',
    "SINGLE='literal \\n # value' # comment",
    'ESCAPED="fixture \\" value" # comment',
    "EMPTY= # comment",
    "HASH=value#suffix"
  ].join("\n"));
  assert.equal(values.get("DOUBLE"), "fixture # value");
  assert.equal(values.get("SINGLE"), "literal \\n # value");
  assert.equal(values.get("ESCAPED"), 'fixture " value');
  assert.equal(values.get("EMPTY"), "");
  assert.equal(values.get("HASH"), "value#suffix");
  for (const source of ['KEY="fixture-secret', "KEY='fixture-secret", 'KEY="fixture-secret" trailing']) {
    assert.throws(() => parseDotEnv(source), (error) => {
      assert.equal(error.message.includes("fixture-secret"), false);
      return true;
    });
  }
});

test("process environment fallback fills missing stores without overwriting file imports", async (t) => {
  const baseDir = await mkdtemp(path.join(tmpdir(), "sorrel-vault-fallback-"));
  t.after(() => rm(baseDir, { recursive: true, force: true }));
  const envKey = "SORREL_VAULT_TEST_FALLBACK";
  const previous = process.env[envKey];
  process.env[envKey] = "fixture-fallback";
  t.after(() => {
    if (previous === undefined) delete process.env[envKey];
    else process.env[envKey] = previous;
  });
  await writeFile(path.join(baseDir, ".env"), `${envKey}=fixture-file\n`);
  const backend = new LocalDevSecretBackend({
    secretRefs: [],
    localDev: {
      backend: "local-dev",
      import: { envFiles: [{ path: ".env", environment: "dev", required: true }], allowProcessEnvFallback: true },
      bindings: [
        { secret: { id: "dev" }, envKey, environment: "dev", storeKey: "dev" },
        { secret: { id: "prod" }, envKey, environment: "prod", storeKey: "prod" }
      ]
    }
  }, { baseDir });
  await backend.importEnvFiles();
  assert.equal(backend.valuesByStoreKey.get("dev"), "fixture-file");
  assert.equal(backend.valuesByStoreKey.get("prod"), "fixture-fallback");
});

test("materialized env retains keys named __proto__", () => {
  const backend = new LocalDevSecretBackend({
    grants: [],
    secretRefs: [{ id: "fixture", name: "__proto__", required: true }],
    localDev: { backend: "local-dev", bindings: [{ secret: { id: "fixture" }, environment: "dev", envKey: "FIXTURE", storeKey: "fixture" }] }
  }, { corePolicy: () => ({ status: "allow" }) });
  backend.importValues({ FIXTURE: "fixture-value" }, "dev");
  const env = backend.materializeEnv([{ secret: "fixture", environment: "dev" }]);
  assert.equal(Object.hasOwn(env, "__proto__"), true);
  assert.equal(env.__proto__, "fixture-value");
  assert.equal(Object.getPrototypeOf(env), Object.prototype);
});

test("binding keys cannot collide across secrets and environments containing colons", () => {
  const backend = new LocalDevSecretBackend({
    grants: [],
    secretRefs: [{ id: "secret_a:b", name: "ONE" }, { id: "secret_a", name: "TWO" }],
    localDev: { backend: "local-dev", bindings: [
      { secret: { id: "secret_a:b" }, environment: "c", envKey: "ONE", storeKey: "one" },
      { secret: { id: "secret_a" }, environment: "b:c", envKey: "TWO", storeKey: "two" }
    ] }
  }, { corePolicy: () => ({ status: "allow" }) });
  backend.importValues({ ONE: "fixture-one", TWO: "fixture-two" }, "*");
  assert.equal(backend.resolve({ secret: "secret_a:b", environment: "c" }).value, "fixture-one");
  assert.equal(backend.resolve({ secret: "secret_a", environment: "b:c" }).value, "fixture-two");
});
