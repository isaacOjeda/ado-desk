// Plans the next release from the Conventional Commits since the latest `v*` tag:
// the next version and the release notes. Used by .github/workflows/release.yml.
//
//   node scripts/release-plan.mjs --notes-out release-notes.md
//
// Prints `version`, `tag` and `release` (true/false) as `key=value` lines and appends them to
// $GITHUB_OUTPUT when it is set. Pure logic is exported for scripts/release-plan.test.mjs.
import { execFileSync } from "node:child_process";
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const HEADER = /^(?<type>[a-z]+)(?:\((?<scope>[^)]+)\))?(?<bang>!)?: (?<description>.+)$/;

/** Splits a commit into its Conventional Commit parts; non-conventional subjects become type `other`. */
export function parseCommit({ hash, subject, body = "" }) {
  const match = HEADER.exec(subject.trim());
  const breakingFooter = /^BREAKING[ -]CHANGE: /m.test(body);
  if (!match) {
    return { hash: hash.slice(0, 7), type: "other", scope: null, breaking: breakingFooter, description: subject.trim() };
  }
  const { type, scope, bang, description } = match.groups;
  return { hash: hash.slice(0, 7), type, scope: scope ?? null, breaking: Boolean(bang) || breakingFooter, description };
}

/**
 * Next semver after `current`: breaking -> major (minor while on 0.x), feat -> minor, anything
 * else -> patch. `null` when there are no commits.
 */
export function nextVersion(current, commits) {
  if (commits.length === 0) return null;
  const [major, minor, patch] = current.split(".").map(Number);
  if (commits.some((c) => c.breaking)) {
    return major === 0 ? `0.${minor + 1}.0` : `${major + 1}.0.0`;
  }
  if (commits.some((c) => c.type === "feat")) return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

const SECTIONS = [
  { title: "Breaking changes", match: (c) => c.breaking },
  { title: "Features", match: (c) => c.type === "feat" },
  { title: "Fixes", match: (c) => c.type === "fix" },
  { title: "Performance", match: (c) => c.type === "perf" },
  { title: "Other changes", match: () => true },
];

function line(commit, withType) {
  const scope = commit.scope ? `**${commit.scope}:** ` : "";
  const type = withType && commit.type !== "other" ? `${commit.type}: ` : "";
  return `- ${type}${scope}${commit.description} (${commit.hash})`;
}

/** Release body: changes grouped by type, a compare link, then the install template. */
export function renderNotes({ version, previousTag, repository, commits, template }) {
  const parts = ["## What's changed", ""];
  const remaining = new Set(commits);
  for (const section of SECTIONS) {
    const items = [...remaining].filter(section.match);
    if (items.length === 0) continue;
    items.forEach((c) => remaining.delete(c));
    const other = section.title === "Other changes";
    parts.push(`### ${section.title}`, "", ...items.map((c) => line(c, other)), "");
  }
  if (previousTag && repository) {
    parts.push(`Full changelog: https://github.com/${repository}/compare/${previousTag}...v${version}`, "");
  }
  if (template.trim()) parts.push("---", "", template.replaceAll("<version>", version).trim(), "");
  return parts.join("\n");
}

function git(...args) {
  return execFileSync("git", args, { encoding: "utf8" }).trim();
}

function latestTag() {
  try {
    return git("describe", "--tags", "--abbrev=0", "--match", "v[0-9]*");
  } catch {
    return null;
  }
}

/** Commits after `tag` (all history when null), oldest first, without merges or release bumps. */
function commitsSince(tag) {
  const range = tag ? [`${tag}..HEAD`] : ["HEAD"];
  const raw = git("log", "--no-merges", "--reverse", "--format=%H%x1f%s%x1f%b%x1e", ...range);
  return raw
    .split("\x1e")
    .map((entry) => entry.trim())
    .filter(Boolean)
    .map((entry) => {
      const [hash, subject, body] = entry.split("\x1f");
      return parseCommit({ hash, subject, body });
    })
    .filter((c) => !(c.type === "chore" && c.scope === "release"));
}

function main(argv) {
  const notesOut = argv[argv.indexOf("--notes-out") + 1];
  if (!argv.includes("--notes-out") || !notesOut) throw new Error("usage: release-plan.mjs --notes-out <file>");

  const tag = latestTag();
  const current = tag
    ? tag.slice(1)
    : JSON.parse(readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8")).version;
  const commits = commitsSince(tag);
  // Without any tag yet, the version in tauri.conf.json is the first release.
  const version = tag ? nextVersion(current, commits) : current;

  const outputs = { release: String(version !== null), version: version ?? "", tag: version ? `v${version}` : "" };
  if (version) {
    const template = readFileSync(new URL("../.github/release-notes.md", import.meta.url), "utf8");
    const notes = renderNotes({ version, previousTag: tag, repository: process.env.GITHUB_REPOSITORY, commits, template });
    writeFileSync(notesOut, notes);
  }
  const lines = Object.entries(outputs).map(([k, v]) => `${k}=${v}`).join("\n") + "\n";
  process.stdout.write(lines);
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, lines);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main(process.argv.slice(2));
}
