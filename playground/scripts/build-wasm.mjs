import { copyFile, mkdir } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const playgroundDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const workspaceDir = path.resolve(playgroundDir, "..");
const release = process.argv.includes("--release");
const cargoArgs = [
  "build",
  "-p",
  "pg-completion-playground-wasm",
  "--target",
  "wasm32-unknown-unknown",
  ...(release ? ["--release"] : []),
];

function run(command, args, options = {}) {
  return spawnSync(command, args, {
    cwd: workspaceDir,
    encoding: "utf8",
    ...options,
  });
}

const build = run("cargo", cargoArgs, { stdio: "inherit" });
if (build.status !== 0) {
  process.exit(build.status ?? 1);
}

const profile = release ? "release" : "debug";
const source = path.join(
  workspaceDir,
  "target",
  "wasm32-unknown-unknown",
  profile,
  "pg_completion_playground_wasm.wasm",
);
const outputDir = path.join(playgroundDir, "src", "generated");
await mkdir(outputDir, { recursive: true });
await copyFile(source, path.join(outputDir, "pg_completion_playground.wasm"));
console.log(`WASM ready (${profile}).`);
