#!/usr/bin/env node
// The `ranma` command of the @masshirodev/ranma npm package: runs the binary
// from the platform package npm installed beside it (optionalDependencies, one
// per os/cpu, as esbuild does). Node cannot exec, so it waits for ranma and
// passes its exit status on; the terminal is ranma's the whole time.
"use strict";
const { spawnSync } = require("node:child_process");
const path = require("node:path");

const platforms = {
  "linux-x64": "@masshirodev/ranma-linux-x64",
  "linux-arm64": "@masshirodev/ranma-linux-arm64",
};
const key = `${process.platform}-${process.arch}`;
const pkg = platforms[key];
if (!pkg) {
  console.error(`ranma: no build for ${key}; ranma runs on Linux, x64 or arm64`);
  process.exit(1);
}
let bin;
try {
  bin = path.join(path.dirname(require.resolve(`${pkg}/package.json`)), "bin", "ranma");
} catch {
  console.error(`ranma: ${pkg} is not installed (installed with --omit=optional?); reinstall @masshirodev/ranma`);
  process.exit(1);
}
// Signals the terminal sends reach ranma directly (same process group); node
// only has to not die of them first.
for (const sig of ["SIGINT", "SIGTERM", "SIGHUP", "SIGQUIT"]) process.on(sig, () => {});
const r = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
if (r.error) {
  console.error(`ranma: ${r.error.message}`);
  process.exit(1);
}
if (r.signal) {
  // Die of the same signal so the caller sees what ranma saw; the handlers
  // above would swallow it, and node would then exit 0.
  process.removeAllListeners(r.signal);
  process.kill(process.pid, r.signal);
  process.exit(128 + (require("node:os").constants.signals[r.signal] ?? 0));
}
process.exit(r.status ?? 1);
