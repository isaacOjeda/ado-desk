// Files tab: changed file list on the left, the selected file's diff (loaded on demand) on the right.
import { memo, useCallback, useMemo, useRef, useState } from "react";
import { Badge, EmptyState, OpenInBrowser, RichText } from "../../components/ui";
import type { PullRequest } from "../../lib/api";
import { useResource, type Resource } from "../../lib/useResource";
import {
  isMarkdownFile,
  previewObjectId,
  pullRequestApi,
  type FileChange,
  type FileDiff,
  type PullRequestFiles,
  type PullRequestThread,
} from "./api";
import { DiffHeader, DiffView, type DiffMode, type LineCommenting } from "./Diff";
import { PullRequestReadOnlyHint } from "./Actions";
import { Async } from "./Async";
import { changeTypeBadge, directory, fileName, threadCountsByPath } from "./format";

type PullRequestRef = Pick<PullRequest, "project" | "repositoryId" | "id" | "url">;

const FileRow = memo(function FileRow({
  file,
  selected,
  comments,
  onSelect,
}: {
  file: FileChange;
  selected: boolean;
  comments: number;
  onSelect: (path: string) => void;
}) {
  const badge = changeTypeBadge(file.changeType);
  const dir = directory(file.path);
  return (
    <li>
      <button
        className={`pr-file${selected ? " is-active" : ""}`}
        onClick={() => onSelect(file.path)}
        title={file.originalPath ? `${file.originalPath} → ${file.path}` : file.path}
        aria-current={selected || undefined}
      >
        <span className={`pr-file-type tone-${badge.tone}`} aria-label={badge.label}>
          {badge.letter}
        </span>
        <span className="pr-file-main">
          <span className="pr-file-name">{fileName(file.path)}</span>
          {dir && <span className="pr-file-dir">{dir}</span>}
        </span>
        {comments > 0 && (
          <span className="pr-file-comments" title={`${comments} comment thread${comments === 1 ? "" : "s"}`}>
            <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden>
              <path
                d="M2 3.5A1.5 1.5 0 0 1 3.5 2h9A1.5 1.5 0 0 1 14 3.5v6a1.5 1.5 0 0 1-1.5 1.5H7l-3 3v-3h-.5A1.5 1.5 0 0 1 2 9.5z"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.3"
              />
            </svg>
            {comments}
          </span>
        )}
      </button>
    </li>
  );
});

function PreviewView({ pr, objectId }: { pr: PullRequestRef; objectId: string }) {
  const { project, repositoryId, id } = pr;
  const preview = useResource(
    useCallback(() => pullRequestApi.filePreview({ project, repositoryId, id }, objectId), [project, repositoryId, id, objectId]),
  );
  return (
    <Async resource={preview}>
      {(data) =>
        data.tooLarge || data.binary ? (
          <div className="diff-message">
            <span>{data.tooLarge ? "This file is too large to preview here." : "Binary file not shown."}</span>
            {data.tooLarge && <OpenInBrowser url={pr.url} />}
          </div>
        ) : (
          <RichText className="diff-preview" html={data.html} />
        )
      }
    </Async>
  );
}

function DiffPanel({
  pr,
  file,
  threads,
  cache,
  commenting,
  readOnly,
}: {
  pr: PullRequestRef;
  file: FileChange;
  threads: PullRequestThread[];
  cache: Map<string, FileDiff>;
  commenting?: LineCommenting;
  readOnly: boolean;
}) {
  const { project, repositoryId, id } = pr;
  const fetcher = useCallback(async () => {
    const cached = cache.get(file.path);
    if (cached) return cached;
    const diff = await pullRequestApi.fileDiff({ project, repositoryId, id }, file);
    cache.set(file.path, diff);
    return diff;
  }, [cache, file, project, repositoryId, id]);
  const diff = useResource(fetcher);
  const fileThreads = useMemo(() => threads.filter((t) => t.kind === "file" && t.filePath === file.path), [threads, file.path]);
  const previewId = isMarkdownFile(file.path) ? previewObjectId(file) : null;
  const [mode, setMode] = useState<DiffMode>("diff");
  const showPreview = previewId !== null && mode === "preview";

  return (
    <div className="pr-diff">
      <DiffHeader file={file} diff={diff.data} mode={previewId ? mode : undefined} onModeChange={setMode} />
      {readOnly && !showPreview && (
        <div className="diff-message">
          <PullRequestReadOnlyHint />
        </div>
      )}
      {showPreview ? (
        <PreviewView pr={pr} objectId={previewId} />
      ) : (
        <Async resource={diff}>
          {(data) => <DiffView file={file} diff={data} threads={fileThreads} webUrl={pr.url} commenting={commenting} />}
        </Async>
      )}
    </div>
  );
}

function FilesPane({
  pr,
  files,
  truncated,
  threads,
  commenting,
  readOnly,
}: {
  pr: PullRequestRef;
  files: FileChange[];
  truncated: boolean;
  threads: PullRequestThread[];
  commenting?: LineCommenting;
  readOnly: boolean;
}) {
  const [selectedPath, setSelectedPath] = useState<string | null>(files[0]?.path ?? null);
  const cache = useRef(new Map<string, FileDiff>()).current;
  const counts = useMemo(() => threadCountsByPath(threads), [threads]);
  const selected = files.find((f) => f.path === selectedPath) ?? files[0];

  if (!selected) return <EmptyState title="No file changes" hint="This pull request does not change any file." />;

  return (
    <div className="pr-files">
      <nav className="pr-file-list" aria-label="Changed files">
        {truncated && (
          <p className="pr-truncated">
            <Badge tone="amber">Partial</Badge> Only the first {files.length} files are listed.
          </p>
        )}
        <ul className="pr-plain-list">
          {files.map((file) => (
            <FileRow
              key={file.path}
              file={file}
              selected={file.path === selected.path}
              comments={counts.get(file.path) ?? 0}
              onSelect={setSelectedPath}
            />
          ))}
        </ul>
      </nav>
      <DiffPanel
        key={selected.path}
        pr={pr}
        file={selected}
        threads={threads}
        cache={cache}
        commenting={commenting}
        readOnly={readOnly}
      />
    </div>
  );
}

export function FilesTab({
  pr,
  files,
  threads,
  commenting,
  readOnly,
}: {
  pr: PullRequestRef;
  files: Resource<PullRequestFiles>;
  threads: PullRequestThread[];
  /** Absent when the detail is read-only or the pull request is closed. */
  commenting?: LineCommenting;
  readOnly: boolean;
}) {
  return (
    <Async resource={files}>
      {(data) => (
        <FilesPane
          pr={pr}
          files={data.files}
          truncated={data.truncated}
          threads={threads}
          commenting={commenting}
          readOnly={readOnly}
        />
      )}
    </Async>
  );
}
