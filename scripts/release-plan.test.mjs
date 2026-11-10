// Run with: pnpm test:scripts
import { test } from "node:test";
import assert from "node:assert/strict";
import { nextVersion, parseCommit, renderNotes } from "./release-plan.mjs";

const commit = (subject, body = "") => parseCommit({ hash: "abcdef1234567", subject, body });

test("parseCommit reads type, scope, breaking marker and description", () => {
  assert.deepEqual(commit("feat(windows): adapt the tray"), {
    hash: "abcdef1",
    type: "feat",
    scope: "windows",
    breaking: false,
    description: "adapt the tray",
  });
  assert.equal(commit("fix!: drop legacy keychain entry").breaking, true);
  assert.equal(commit("refactor: x", "BREAKING CHANGE: settings moved").breaking, true);
  assert.equal(commit("Update README").type, "other");
});

test("nextVersion bumps patch for fixes and other changes", () => {
  assert.equal(nextVersion("0.1.1", [commit("fix: crash"), commit("docs: typo")]), "0.1.2");
  assert.equal(nextVersion("0.1.1", [commit("Update README")]), "0.1.2");
});

test("nextVersion bumps minor for features", () => {
  assert.equal(nextVersion("0.1.1", [commit("fix: a"), commit("feat: b")]), "0.2.0");
});

test("nextVersion bumps major for breaking changes, minor while on 0.x", () => {
  assert.equal(nextVersion("1.4.2", [commit("feat!: new store")]), "2.0.0");
  assert.equal(nextVersion("0.4.2", [commit("feat!: new store")]), "0.5.0");
});

test("nextVersion returns null when there is nothing to release", () => {
  assert.equal(nextVersion("0.1.1", []), null);
});

test("renderNotes groups changes by type and fills the install template", () => {
  const notes = renderNotes({
    version: "0.2.0",
    previousTag: "v0.1.1",
    repository: "owner/repo",
    commits: [commit("feat(windows): adapt the tray"), commit("fix: crash on start"), commit("ci: add checks")],
    template: "Download `ADO.Desk_<version>_x64-setup.exe`.",
  });
  assert.match(notes, /^## What's changed$/m);
  assert.match(notes, /### Features\n\n- \*\*windows:\*\* adapt the tray \(abcdef1\)/);
  assert.match(notes, /### Fixes\n\n- crash on start \(abcdef1\)/);
  assert.match(notes, /### Other changes\n\n- ci: add checks \(abcdef1\)/);
  assert.match(notes, /Full changelog: https:\/\/github\.com\/owner\/repo\/compare\/v0\.1\.1\.\.\.v0\.2\.0/);
  assert.match(notes, /Download `ADO\.Desk_0\.2\.0_x64-setup\.exe`\./);
  assert.ok(notes.indexOf("### Features") < notes.indexOf("### Fixes"));
});

test("renderNotes lists breaking changes first", () => {
  const notes = renderNotes({
    version: "0.2.0",
    previousTag: "v0.1.1",
    repository: "owner/repo",
    commits: [commit("fix: small"), commit("feat!: new settings format")],
    template: "",
  });
  assert.ok(notes.indexOf("### Breaking changes") < notes.indexOf("### Fixes"));
  assert.match(notes, /### Breaking changes\n\n- new settings format \(abcdef1\)/);
});
