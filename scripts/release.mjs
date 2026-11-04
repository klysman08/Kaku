#!/usr/bin/env node
// Bump the version everywhere, commit, tag and push — the tag triggers the release workflow.
//
//   pnpm release patch|minor|major|<x.y.z>   (add --dry-run to preview, --no-push to stay local)
//
// package.json is the source of truth; tauri.conf.json reads it via "../package.json";
// src-tauri/Cargo.toml (+ Cargo.lock) are kept in sync here.
import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const args = process.argv.slice(2);
const bump = args.find((a) => !a.startsWith("--"));
const dryRun = args.includes("--dry-run");
const push = !args.includes("--no-push");

const run = (cmd, cmdArgs) => execFileSync(cmd, cmdArgs, { cwd: root, stdio: "inherit" });
const git = (...a) => execFileSync("git", a, { cwd: root, encoding: "utf8" }).trim();

if (!bump) {
  console.error("usage: pnpm release <patch|minor|major|x.y.z> [--dry-run] [--no-push]");
  process.exit(1);
}

function nextVersion(current, kind) {
  if (/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(kind)) return kind;
  const [maj, min, pat] = current.split("-")[0].split(".").map(Number);
  if (kind === "major") return `${maj + 1}.0.0`;
  if (kind === "minor") return `${maj}.${min + 1}.0`;
  if (kind === "patch") return `${maj}.${min}.${pat + 1}`;
  throw new Error(`invalid bump "${kind}"`);
}

const pkgPath = resolve(root, "package.json");
const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
const version = nextVersion(pkg.version, bump);
const tag = `v${version}`;

if (git("status", "--porcelain")) {
  console.error("Working tree is not clean. Commit or stash your changes first.");
  process.exit(1);
}
const branch = git("rev-parse", "--abbrev-ref", "HEAD");
if (branch !== "main") console.warn(`warning: releasing from "${branch}", not main`);
if (git("tag", "--list", tag)) {
  console.error(`Tag ${tag} already exists.`);
  process.exit(1);
}

console.log(`${pkg.version} → ${version}${dryRun ? " (dry run)" : ""}`);
if (dryRun) process.exit(0);

pkg.version = version;
writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");

const cargoPath = resolve(root, "src-tauri", "Cargo.toml");
const cargo = readFileSync(cargoPath, "utf8");
const updatedCargo = cargo.replace(/^(\[package\][\s\S]*?^version\s*=\s*")[^"]+(")/m, `$1${version}$2`);
if (updatedCargo === cargo) throw new Error("could not update version in Cargo.toml");
writeFileSync(cargoPath, updatedCargo);

// Refresh Cargo.lock for our own package only (no dependency upgrades).
execFileSync("cargo", ["update", "--workspace", "--offline"], { cwd: resolve(root, "src-tauri"), stdio: "inherit" });

run("git", ["add", "package.json", "src-tauri/Cargo.toml", "src-tauri/Cargo.lock"]);
run("git", ["commit", "-m", `chore(release): ${tag}`]);
run("git", ["tag", "-a", tag, "-m", `KokoroKaku ${tag}`]);

if (push) {
  run("git", ["push", "--follow-tags", "origin", branch]);
  console.log(`\nPushed ${tag}. The Release workflow will build installers and draft a GitHub release.`);
} else {
  console.log(`\nCreated ${tag} locally. Push with: git push --follow-tags origin ${branch}`);
}
