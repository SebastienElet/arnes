#!/usr/bin/env bun
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { existsSync, readFileSync } from "node:fs";
import packageManifest from "../package.json";

const variants = { "darwin-arm64": "darwin-arm64", "linux-x64": "linux-x64-gnu" };
const suffix = variants[`${process.platform}-${process.arch}`];
try {
  if (!suffix) throw new Error(`Unsupported platform: ${process.platform}-${process.arch}`);
  if (process.platform === "linux" && !process.report.getReport().header.glibcVersionRuntime) {
    throw new Error("Linux requires glibc");
  }
  let executable;
  try {
    const manifest = fileURLToPath(import.meta.resolve(`@sebastienelet/arnes-${suffix}/package.json`));
    executable = join(dirname(manifest), "bin/arnes");
    if (JSON.parse(readFileSync(manifest, "utf8")).version !== packageManifest.version || !existsSync(executable)) {
      throw new Error();
    }
  } catch {
    throw new Error(`Missing @sebastienelet/arnes-${suffix}@${packageManifest.version}; install optional dependencies with Bun`);
  }
  const result = Bun.spawnSync([executable, ...process.argv.slice(2)], { stdin: "inherit", stdout: "inherit", stderr: "inherit" });
  if (result.error) throw result.error;
  if (result.signalCode) process.kill(process.pid, result.signalCode);
  process.exit(result.exitCode ?? 1);
} catch (error) {
  console.error(`arnes: ${error.message}`);
  process.exit(1);
}
