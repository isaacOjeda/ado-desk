// Presentational unified diff with inline file threads. File contents are plain
// text and are rendered as React text nodes only (never as HTML).
import { Fragment, useMemo } from "react";
import { Badge, OpenInBrowser } from "../../components/ui";
import type { DiffHunk, DiffLine, FileChange, FileDiff, PullRequestThread } from "./api";
import { changeTypeBadge } from "./format";
import { ThreadCard } from "./Threads";

interface Anchors {
  right: Map<number, PullRequestThread[]>;
  left: Map<number, PullRequestThread[]>;
  /** Threads whose line is not part of the shown hunks: rendered at the top of the file. */
  top: PullRequestThread[];
}

function push(map: Map<number, PullRequestThread[]>, line: number, thread: PullRequestThread) {
  const list = map.get(line);
  if (list) list.push(thread);
  else map.set(line, [thread]);
}

function anchorThreads(hunks: DiffHunk[], threads: PullRequestThread[]): Anchors {
  const newLines = new Set<number>();
  const oldLines = new Set<number>();
  for (const hunk of hunks) {
    for (const line of hunk.lines) {
      if (line.kind !== "delete" && line.newLine) newLines.add(line.newLine);
      if (line.kind !== "add" && line.oldLine) oldLines.add(line.oldLine);
    }
  }
  const anchors: Anchors = { right: new Map(), left: new Map(), top: [] };
  for (const thread of threads) {
    if (thread.rightLine && newLines.has(thread.rightLine)) push(anchors.right, thread.rightLine, thread);
    else if (!thread.rightLine && thread.leftLine && oldLines.has(thread.leftLine))
      push(anchors.left, thread.leftLine, thread);
    else anchors.top.push(thread);
  }
  return anchors;
}

function threadsAfter(line: DiffLine, anchors: Anchors): PullRequestThread[] {
  const right = line.kind !== "delete" && line.newLine ? (anchors.right.get(line.newLine) ?? []) : [];
  const left = line.kind !== "add" && line.oldLine ? (anchors.left.get(line.oldLine) ?? []) : [];
  return left.length ? [...right, ...left] : right;
}

const MARKERS = { add: "+", delete: "-", context: " " } as const;

function InlineThreads({ threads }: { threads: PullRequestThread[] }) {
  return (
    <div className="diff-threads">
      {threads.map((thread) => (
        <ThreadCard key={thread.id} thread={thread} />
      ))}
    </div>
  );
}

function Hunk({ hunk, anchors }: { hunk: DiffHunk; anchors: Anchors }) {
  return (
    <div className="diff-hunk">
      <div className="diff-row diff-hunk-header">
        <span className="diff-gutter" />
        <span className="diff-gutter" />
        <span className="diff-code">
          @@ -{hunk.oldStart},{hunk.oldLines} +{hunk.newStart},{hunk.newLines} @@
        </span>
      </div>
      {hunk.lines.map((line, index) => {
        const threads = threadsAfter(line, anchors);
        return (
          <Fragment key={index}>
            <div className={`diff-row diff-${line.kind}`}>
              <span className="diff-gutter">{line.oldLine ?? ""}</span>
              <span className="diff-gutter">{line.newLine ?? ""}</span>
              <span className="diff-code">
                <span className="diff-marker">{MARKERS[line.kind]}</span>
                {line.text}
              </span>
            </div>
            {threads.length > 0 && <InlineThreads threads={threads} />}
          </Fragment>
        );
      })}
    </div>
  );
}

function emptyMessage(diff: FileDiff, file: FileChange): string | null {
  if (diff.tooLarge) return "This file is too large to display here.";
  if (diff.binary) return "Binary file not shown.";
  if (diff.hunks.length > 0) return null;
  if (file.changeType === "add") return "Empty file added.";
  if (file.changeType === "delete") return "Empty file deleted.";
  if (file.changeType === "rename") return "File renamed without content changes.";
  return "No content changes.";
}

export function DiffView({
  file,
  diff,
  threads,
  webUrl,
}: {
  file: FileChange;
  diff: FileDiff;
  threads: PullRequestThread[];
  webUrl: string;
}) {
  const anchors = useMemo(() => anchorThreads(diff.hunks, threads), [diff, threads]);
  const message = emptyMessage(diff, file);
  return (
    <>
      {anchors.top.length > 0 && <InlineThreads threads={anchors.top} />}
      {message ? (
        <div className="diff-message">
          <span>{message}</span>
          {diff.tooLarge && <OpenInBrowser url={webUrl} />}
        </div>
      ) : (
        <div className="diff-scroll">
          <div className="diff-table">
            {diff.hunks.map((hunk, index) => (
              <Hunk key={index} hunk={hunk} anchors={anchors} />
            ))}
          </div>
        </div>
      )}
    </>
  );
}

export function DiffHeader({ file, diff }: { file: FileChange; diff: FileDiff | null }) {
  const badge = changeTypeBadge(file.changeType);
  return (
    <header className="diff-header">
      <Badge tone={badge.tone}>{badge.label}</Badge>
      <span className="pr-mono diff-header-path" title={file.path}>
        {file.originalPath ? `${file.originalPath} → ${file.path}` : file.path}
      </span>
      {diff && !diff.binary && !diff.tooLarge && (
        <span className="diff-stats">
          <span className="diff-stat-add">+{diff.additions}</span>
          <span className="diff-stat-del">−{diff.deletions}</span>
        </span>
      )}
    </header>
  );
}
