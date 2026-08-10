import Ajv2020 from "ajv/dist/2020.js";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { loadSecretSpec, validateSecretSpecSemantics } from "./lib/spec-loader.mjs";

const root = process.cwd();
const schemaPath = path.join(root, "schemas", "sorrel-secrets.schema.json");
const examplesDir = path.join(root, "examples");
const args = new Set(process.argv.slice(2));

const schema = JSON.parse(await readFile(schemaPath, "utf8"));
const ajv = new Ajv2020({
  allErrors: true,
  strict: true
});
const validate = ajv.compile(schema);

if (args.has("--schema-only")) {
  console.log(`Compiled schema: ${path.relative(root, schemaPath)}`);
  process.exit(0);
}

if (!args.has("--examples-only") && args.size > 0) {
  console.error(`Unknown arguments: ${Array.from(args).join(", ")}`);
  process.exit(2);
}

const files = (await readdir(examplesDir))
  .filter((file) => /^sorrel\.secrets\..+\.ya?ml$/.test(file))
  .sort();

let hasFailure = false;

for (const file of files) {
  const filePath = path.join(examplesDir, file);
  const data = await loadSecretSpec(filePath);

  if (!validate(data)) {
    hasFailure = true;
    console.error(`not ok ${path.relative(root, filePath)}`);
    console.error(JSON.stringify(validate.errors, null, 2));
    continue;
  }

  const semanticErrors = validateSecretSpecSemantics(data, {
    filePath: path.relative(root, filePath)
  });

  if (semanticErrors.length > 0) {
    hasFailure = true;
    console.error(`not ok ${path.relative(root, filePath)}`);
    console.error(semanticErrors.join("\n"));
    continue;
  }

  console.log(`ok ${path.relative(root, filePath)}`);
}

if (hasFailure) {
  process.exit(1);
}

console.log(`Validated ${files.length} sorrel.secrets.yml example(s).`);
