// Unified diff with inline file threads and line selection for new comments. File contents
// are plain text and are rendered as React text nodes only (never as HTML).
import { Fragment, useMemo, useState } from "react";
import { Badge, OpenInBrowser } from "../../components/ui";
import { MAX_COMMENT_CHARS, type DiffHunk, type DiffLine, type DiffSide, type FileChange, type FileDiff, type LineSelection, type PullRequestThread } from "./api";
import { usePullRequestWrite } from "./Actions";
import { changeTypeBadge } from "./format";
import { ThreadCard, type ThreadActions } from "./Threads";

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

function InlineThreads({ threads, actions }: { threads: PullRequestThread[]; actions?: ThreadActions }) {
  return (
    <div className="diff-threads">
      {threads.map((thread) => (
        <ThreadCard key={thread.id} thread={thread} actions={actions} />
      ))}
    </div>
  );
}

/** Diff writes (comments on selected lines, thread replies and resolve); absent when the detail is read-only or the PR is closed. */
export interface LineCommenting {
  onComment: (selection: LineSelection, content: string) => Promise<void>;
  threads: ThreadActions;
  onChanged: () => void;
  onDenied: () => void;
}

/** Lines picked in one hunk: `anchor` is the first click, `focus` the shift-click end. */
interface PickedLines {
  hunk: number;
  side: DiffSide;
  anchor: number;
  focus: number;
}

function sideLine(line: DiffLine, side: DiffSide): number | null {
  if (side === "left") return line.kind !== "add" ? line.oldLine : null;
  return line.kind !== "delete" ? line.newLine : null;
}

/** Indexes of the picked lines that exist on the picked side, in order. */
function pickedIndexes(hunk: DiffHunk, picked: PickedLines): number[] {
  const from = Math.min(picked.anchor, picked.focus);
  const to = Math.max(picked.anchor, picked.focus);
  const indexes: number[] = [];
  for (let i = from; i <= to; i++) if (sideLine(hunk.lines[i], picked.side) !== null) indexes.push(i);
  return indexes;
}

function toSelection(path: string, hunk: DiffHunk, picked: PickedLines): LineSelection | null {
  const indexes = pickedIndexes(hunk, picked);
  if (indexes.length === 0) return null;
  const first = hunk.lines[indexes[0]];
  const last = hunk.lines[indexes[indexes.length - 1]];
  return {
    filePath: path,
    side: picked.side,
    startLine: sideLine(first, picked.side)!,
    endLine: sideLine(last, picked.side)!,
    endOffset: Math.min([...last.text].length + 1, 100_000),
  };
}

function LineCommentBox({
  selection,
  commenting,
  onDone,
}: {
  selection: LineSelection;
  commenting: LineCommenting;
  onDone: () => void;
}) {
  const [text, setText] = useState("");
  const { busy, error, run } = usePullRequestWrite(commenting.onChanged, commenting.onDenied);
  const lines =
    selection.startLine === selection.endLine
      ? `line ${selection.startLine}`
      : `lines ${selection.startLine}–${selection.endLine}`;
  const tooLong = [...text.trim()].length > MAX_COMMENT_CHARS;

  const save = async () => {
    if (busy || text.trim() === "" || tooLong) return;
    if (await run(() => commenting.onComment(selection, text))) onDone();
  };

  return (
    <div className="diff-comment-box">
      <label className="diff-comment-label" htmlFor="diff-comment">
        Comment on {selection.side === "left" ? "original " : ""}
        {lines}
      </label>
      <textarea
        id="diff-comment"
        autoFocus
        value={text}
        disabled={busy}
        placeholder="Write a comment (Markdown)…"
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) void save();
          if (e.key === "Escape") {
            e.stopPropagation();
            onDone();
          }
        }}
      />
      <div className="diff-comment-footer">
        <button className="btn btn-sm btn-primary" onClick={() => void save()} disabled={busy || text.trim() === "" || tooLong}>
          {busy ? "Saving…" : "Comment"}
        </button>
        <button className="btn btn-sm btn-ghost" onClick={onDone} disabled={busy}>
          Cancel
        </button>
        {tooLong && <span className="pr-action-error">At most {MAX_COMMENT_CHARS} characters.</span>}
        {error && (
          <span className="pr-action-error" role="alert">
            {error}
          </span>
        )}
      </div>
    </div>
  );
}

function Gutter({
  number,
  side,
  onPick,
}: {
  number: number | null;
  side: DiffSide;
  onPick?: (side: DiffSide, extend: boolean) => void;
}) {
  if (number === null || !onPick) return <span className="diff-gutter">{number ?? ""}</span>;
  return (
    <button
      className="diff-gutter diff-gutter-pick"
      title="Comment on this line (Shift+click to select a range)"
      onClick={(e) => onPick(side, e.shiftKey)}
    >
      {number}
    </button>
  );
}

function Hunk({
  hunk,
  hunkIndex,
  anchors,
  path,
  picked,
  onPick,
  commenting,
  onClearPick,
}: {
  hunk: DiffHunk;
  hunkIndex: number;
  anchors: Anchors;
  path: string;
  picked: PickedLines | null;
  onPick: (hunk: number, line: number, side: DiffSide, extend: boolean) => void;
  commenting?: LineCommenting;
  onClearPick: () => void;
}) {
  const mine = picked?.hunk === hunkIndex ? picked : null;
  const indexes = mine ? pickedIndexes(hunk, mine) : [];
  const selected = new Set(indexes);
  const boxAfter = indexes.length ? indexes[indexes.length - 1] : -1;
  const selection = mine && commenting ? toSelection(path, hunk, mine) : null;

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
        const pick = commenting
          ? (side: DiffSide, extend: boolean) => onPick(hunkIndex, index, side, extend)
          : undefined;
        return (
          <Fragment key={index}>
            <div className={`diff-row diff-${line.kind}${selected.has(index) ? " is-picked" : ""}`}>
              <Gutter number={sideLine(line, "left")} side="left" onPick={pick} />
              <Gutter number={sideLine(line, "right")} side="right" onPick={pick} />
              <span className="diff-code">
                <span className="diff-marker">{MARKERS[line.kind]}</span>
                {line.text}
              </span>
            </div>
            {threads.length > 0 && <InlineThreads threads={threads} actions={commenting?.threads} />}
            {index === boxAfter && selection && commenting && (
              <LineCommentBox
                key={`${selection.side}:${selection.startLine}`}
                selection={selection}
                commenting={commenting}
                onDone={onClearPick}
              />
            )}
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
  commenting,
}: {
  file: FileChange;
  diff: FileDiff;
  threads: PullRequestThread[];
  webUrl: string;
  commenting?: LineCommenting;
}) {
  const anchors = useMemo(() => anchorThreads(diff.hunks, threads), [diff, threads]);
  const message = emptyMessage(diff, file);
  const [picked, setPicked] = useState<PickedLines | null>(null);

  const onPick = (hunk: number, line: number, side: DiffSide, extend: boolean) => {
    // Shift+click extends within the same hunk and side; anything else starts a new pick.
    if (extend && picked && picked.hunk === hunk && picked.side === side) setPicked({ ...picked, focus: line });
    else setPicked({ hunk, side, anchor: line, focus: line });
  };
  return (
    <>
      {anchors.top.length > 0 && <InlineThreads threads={anchors.top} actions={commenting?.threads} />}
      {message ? (
        <div className="diff-message">
          <span>{message}</span>
          {diff.tooLarge && <OpenInBrowser url={webUrl} />}
        </div>
      ) : (
        <div className="diff-scroll">
          <div className="diff-table">
            {diff.hunks.map((hunk, index) => (
              <Hunk
                key={index}
                hunk={hunk}
                hunkIndex={index}
                anchors={anchors}
                path={file.path}
                picked={picked}
                onPick={onPick}
                commenting={commenting}
                onClearPick={() => setPicked(null)}
              />
            ))}
          </div>
        </div>
      )}
    </>
  );
}

export type DiffMode = "diff" | "preview";

export function DiffHeader({
  file,
  diff,
  mode,
  onModeChange,
}: {
  file: FileChange;
  diff: FileDiff | null;
  /** Set only for files that have a preview (Markdown). */
  mode?: DiffMode;
  onModeChange?: (mode: DiffMode) => void;
}) {
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
      {mode && onModeChange && (
        <span className="segmented" role="group" aria-label="View">
          {(["diff", "preview"] as const).map((m) => (
            <button key={m} className={mode === m ? "is-active" : ""} aria-pressed={mode === m} onClick={() => onModeChange(m)}>
              {m === "diff" ? "Diff" : "Preview"}
            </button>
          ))}
        </span>
      )}
    </header>
  );
}
