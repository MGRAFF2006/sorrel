# Validation

Sorrel protocol examples and produced objects should be validated against the
versioned JSON Schema bundle before they are persisted, exchanged with another
process, or accepted from untrusted input.

## Local validation

From this package directory:

```bash
npm ci
npm run validate
```

The package scripts use `ajv` with JSON Schema Draft 2020-12 support:

```bash
npm run validate:schemas
npm run validate:examples
```

`examples/*.json` files are positive fixtures and must validate. Files under
`examples/invalid/*.json` are negative fixtures and must be rejected by the same
schema. Keep both fixture sets small and focused when adding protocol fields.

## Direct validator usage

If you do not want to use npm scripts, invoke the local validator directly:

```bash
node scripts/validate.mjs --schema-only
node scripts/validate.mjs --examples-only
```

To validate a single object produced by another tool:

```bash
node -e '
import Ajv2020 from "ajv/dist/2020.js";
import { readFileSync } from "node:fs";

const schema = JSON.parse(readFileSync("schemas/sorrel-object.schema.json", "utf8"));
const data = JSON.parse(readFileSync("path/to/object.json", "utf8"));
const validate = new Ajv2020({ allErrors: true, strict: true }).compile(schema);

if (!validate(data)) {
  console.error(validate.errors);
  process.exit(1);
}
'
```

## CI guidance

Run `npm run validate` whenever schema files, examples, SDK encoders, or SDK
decoders change. CI should treat validation failures as contract failures.

Consumers should validate in two places:

1. At ingress, before accepting objects from other processes or networks.
2. At persistence boundaries, before writing durable objects or exchanging them
   with another Sorrel component.

## SDK implementer notes

- Preserve unknown `metadata` keys, but do not rely on them for protocol
  behavior.
- Reject unknown `schemaVersion` values by default.
- Resolve object references by both `kind` and `id`; do not assume an `id`
  prefix is sufficient to prove the target type.
- Treat `Principal`, `Capability`, `ResourceRef`, `Grant`, `Policy`,
  `PolicyDecision`, and `AuditEvent` as portable Core authorization records.
  Hub may consume, render, search, and route these records, but it does not own
  the permission spine or become an identity provider.
- Never serialize raw secret values into `SecretRef`, `Workflow`, `Runner`,
  `Grant`, `Policy`, `PolicyDecision`, `AuditEvent`, or `AgentPolicy` objects.
  Only handles, grants, decisions, audit records, and redaction metadata belong
  in the protocol.
