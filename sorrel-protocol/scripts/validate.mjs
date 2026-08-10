import Ajv2020 from "ajv/dist/2020.js";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";

const root = process.cwd();
const schemaPath = path.join(root, "schemas", "sorrel-object.schema.json");
const examplesDir = path.join(root, "examples");
const invalidExamplesDir = path.join(examplesDir, "invalid");
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

async function jsonFiles(dir) {
  try {
    return (await readdir(dir))
      .filter((file) => file.endsWith(".json"))
      .sort();
  } catch (error) {
    if (error && error.code === "ENOENT") {
      return [];
    }

    throw error;
  }
}

const files = await jsonFiles(examplesDir);
const invalidFiles = await jsonFiles(invalidExamplesDir);

let hasFailure = false;

for (const file of files) {
  const filePath = path.join(examplesDir, file);
  const data = JSON.parse(await readFile(filePath, "utf8"));

  if (validate(data)) {
    console.log(`ok ${path.relative(root, filePath)}`);
    continue;
  }

  hasFailure = true;
  console.error(`not ok ${path.relative(root, filePath)}`);
  console.error(JSON.stringify(validate.errors, null, 2));
}

for (const file of invalidFiles) {
  const filePath = path.join(invalidExamplesDir, file);
  const data = JSON.parse(await readFile(filePath, "utf8"));

  if (!validate(data)) {
    console.log(`ok ${path.relative(root, filePath)} rejected`);
    continue;
  }

  hasFailure = true;
  console.error(`not ok ${path.relative(root, filePath)} unexpectedly validated`);
}

if (hasFailure) {
  process.exit(1);
}

console.log(`Validated ${files.length} example object(s); rejected ${invalidFiles.length} invalid example object(s).`);
