#!/usr/bin/env node
import process from "node:process";
import {
  CliError,
  DEFAULT_SPEC_PATH,
  evaluateGrant,
  importEnv,
  listSecretRefs,
  loadSpecForCli,
  parseArgs,
  readTextInput,
  redactInput
} from "./lib/cli.mjs";

const HELP = `sorrel-vault CLI — compose the local vault library modules.

Usage:
  node scripts/vault-cli.mjs <command> [options]

Commands:
  list      List declared SecretRef handles and the environments that define them.
  grant     Evaluate access for a principal/secret/environment/action.
  import    Import a .env file into the local backend (reports keys only).
  redact    Redact text from a file or stdin using the spec redaction policy.

Global options:
  --spec <path>        Secret spec path (default: ${DEFAULT_SPEC_PATH}).
  --env <name>         Environment name filter/target.
  --help               Show this help.

Command options:
  grant   --secret <secretRefId> --action <read|inject|materialize|redact>
          --principal <id | Kind:id>   (alias: --actor)
  import  --file <path>   (optional; defaults to spec-declared envFiles)
  redact  --file <path>   (or pipe text via stdin)

Notes:
  Raw secret values are never printed. list/grant report handles and decisions
  only; import reports bound keys; redact masks known secret material in output.
`;

function getSpecPath(flags) {
  return flags.spec ?? DEFAULT_SPEC_PATH;
}

function getPrincipal(flags) {
  return flags.principal ?? flags.actor;
}

async function runList(flags) {
  const { spec } = await loadSpecForCli({ specPath: getSpecPath(flags) });
  const result = listSecretRefs(spec, { environment: flags.env });

  console.log(`SecretRef handles (${result.count}):`);
  for (const ref of result.secretRefs) {
    const envs =
      ref.boundEnvironments.length > 0 ? ref.boundEnvironments.join(", ") : ref.environment;
    console.log(`- ${ref.id} (${ref.name}) [${ref.valueType ?? "unknown"}]`);
    console.log(`    environments: ${envs}`);
    console.log(`    required: ${ref.required}`);
    if (ref.grantEnvironments.length > 0) {
      console.log(`    granted in: ${ref.grantEnvironments.join(", ")}`);
    }
  }
  console.log("(values are never shown)");
}

async function runGrant(flags) {
  const { spec } = await loadSpecForCli({ specPath: getSpecPath(flags) });
  const result = evaluateGrant(spec, {
    secret: flags.secret,
    environment: flags.env,
    action: flags.action ?? "read",
    principal: getPrincipal(flags)
  });

  console.log(`Decision: ${result.status}`);
  console.log(`  secret: ${result.secret}`);
  console.log(`  environment: ${result.environment}`);
  console.log(`  action: ${result.action} (${result.capability})`);
  if (result.grant) {
    console.log(`  grant: ${result.grant}`);
  }
  if (result.reason) {
    console.log(`  reason: ${result.reason}`);
  }

  if (!result.allowed) {
    process.exitCode = 2;
  }
}

async function runImport(flags) {
  const { spec, baseDir } = await loadSpecForCli({ specPath: getSpecPath(flags) });
  const result = await importEnv(spec, {
    baseDir,
    environment: flags.env,
    file: flags.file
  });

  console.log(`Imported ${result.importedCount} key(s) into the local backend:`);
  for (const key of result.importedKeys) {
    console.log(`- ${key.envKey} -> ${key.secret} (${key.environment})`);
  }
  console.log("(raw values are not printed or persisted)");
}

async function runRedact(flags) {
  const { spec, baseDir } = await loadSpecForCli({ specPath: getSpecPath(flags) });
  const text = await readTextInput({
    file: flags.file,
    stdin: flags.file ? undefined : process.stdin
  });

  const result = await redactInput(spec, text, {
    baseDir,
    environment: flags.env,
    principal: getPrincipal(flags)
  });

  process.stdout.write(result.redacted.endsWith("\n") ? result.redacted : `${result.redacted}\n`);
}

async function main() {
  const { command, flags } = parseArgs(process.argv.slice(2));

  if (flags.help || !command) {
    console.log(HELP);
    return;
  }

  switch (command) {
    case "list":
      await runList(flags);
      break;
    case "grant":
      await runGrant(flags);
      break;
    case "import":
      await runImport(flags);
      break;
    case "redact":
      await runRedact(flags);
      break;
    default:
      throw new CliError(`Unknown command: ${command}\nRun with --help for usage.`);
  }
}

main().catch((error) => {
  const exitCode = error instanceof CliError ? error.exitCode : 1;
  console.error(`error: ${error.message}`);
  process.exitCode = exitCode || 1;
});
