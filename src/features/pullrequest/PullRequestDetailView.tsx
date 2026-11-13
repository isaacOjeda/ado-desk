// Container for the pull request detail: loads data, owns tab state and the write hooks.
// Layout mirrors the Azure DevOps web pull request page (Overview / Files).
import { useCallback, useMemo, useState } from "react";
import { Badge, DetailHeader, OpenInBrowser } from "../../components/ui";
import type { PullRequest } from "../../lib/api";
import { timeAgo } from "../../lib/format";
import { useResource } from "../../lib/useResource";
import { pullRequestApi, type LineSelection, type PullRequestThread, type ThreadStatusChange } from "./api";
import { checksApi } from "./checks";
import { FilesTab } from "./Files";
import type { LineCommenting } from "./Diff";
import { statusBadge } from "./format";
import { Overview, type PullRequestWrites } from "./Overview";
import type { ThreadActions } from "./Threads";
import "./pullrequest.css";
import { AskAssistant } from "../assistant/AskAssistant";
import { pullRequestPrompt } from "../assistant/events";

export interface PullRequestDetailViewProps {
  /** Row the user clicked; use it to render the header instantly while details load. */
  pullRequest: PullRequest;
  onBack: () => void;
}

type Tab = "overview" | "files";

const NO_THREADS: PullRequestThread[] = [];

export function PullRequestDetailView({ pullRequest, onBack }: PullRequestDetailViewProps) {
  const { project, repositoryId, id } = pullRequest;
  const [tab, setTab] = useState<Tab>("overview");

  const detail = useResource(useCallback(() => pullRequestApi.detail({ project, repositoryId, id }), [project, repositoryId, id]));
  const threads = useResource(useCallback(() => pullRequestApi.threads({ project, repositoryId, id }), [project, repositoryId, id]));
  const files = useResource(useCallback(() => pullRequestApi.files({ project, repositoryId, id }), [project, repositoryId, id]));
  const checks = useResource(useCallback(() => checksApi.detail({ project, repositoryId, id }), [project, repositoryId, id]));

  // A refused write switches this detail to read-only; it is never remembered elsewhere.
  const [readOnly, setReadOnly] = useState(false);
  const reloadDetail = detail.reload;
  const reloadThreads = threads.reload;
  const writes: PullRequestWrites = useMemo(
    () => ({
      readOnly,
      onVote: (vote: number) => pullRequestApi.vote({ project, repositoryId, id }, vote),
      onLineComment: (selection: LineSelection, content: string) =>
        pullRequestApi.lineComment({ project, repositoryId, id }, selection, content),
      onChanged: () => {
        reloadDetail();
        reloadThreads();
      },
      onDenied: () => setReadOnly(true),
    }),
    [readOnly, project, repositoryId, id, reloadDetail, reloadThreads],
  );

  // Replies and status changes only touch the threads.
  const threadActions: ThreadActions = useMemo(
    () => ({
      onReply: (thread: PullRequestThread, parentCommentId: number, content: string) =>
        pullRequestApi.reply({ project, repositoryId, id }, thread.id, parentCommentId, content),
      onStatus: (thread: PullRequestThread, status: ThreadStatusChange) =>
        pullRequestApi.threadStatus({ project, repositoryId, id }, thread.id, status),
      onChanged: reloadThreads,
      onDenied: () => setReadOnly(true),
    }),
    [project, repositoryId, id, reloadThreads],
  );
  const lineCommenting: LineCommenting = useMemo(
    () => ({ onComment: writes.onLineComment, threads: threadActions, onChanged: reloadThreads, onDenied: writes.onDenied }),
    [writes, threadActions, reloadThreads],
  );
  // Only open pull requests take new comments, replies or status changes; wait for the detail to know.
  const detailStatus = detail.data?.status;
  const canComment = !readOnly && (detailStatus === "active" || detailStatus === "draft");

  const status = statusBadge(detail.data?.status ?? (pullRequest.isDraft ? "draft" : "active"));
  const fileCount = files.data?.files.length;

  return (
    <div className="detail">
      <DetailHeader
        onBack={onBack}
        eyebrow={
          <>
            <span>!{pullRequest.id}</span>
            <Badge tone={status.tone}>{status.label}</Badge>
          </>
        }
        title={detail.data?.title ?? pullRequest.title}
        actions={
          <>
            <AskAssistant prompt={pullRequestPrompt(pullRequest.id)} />
            <OpenInBrowser url={pullRequest.url} />
          </>
        }
      >
        <div className="detail-meta">
          <span>
            {detail.data?.author ?? pullRequest.author} wants to merge{" "}
            <code className="pr-branch">{pullRequest.sourceBranch}</code> into{" "}
            <code className="pr-branch">{pullRequest.targetBranch}</code>
          </span>
          <span>{pullRequest.repository}</span>
          <time dateTime={pullRequest.createdAt} title={new Date(pullRequest.createdAt).toLocaleString()}>
            Created {timeAgo(pullRequest.createdAt)}
          </time>
        </div>
        <nav className="tabs" role="tablist">
          <button role="tab" aria-selected={tab === "overview"} className={`tab${tab === "overview" ? " is-active" : ""}`} onClick={() => setTab("overview")}>
            Overview
          </button>
          <button role="tab" aria-selected={tab === "files"} className={`tab${tab === "files" ? " is-active" : ""}`} onClick={() => setTab("files")}>
            Files
            {fileCount !== undefined && <span className="tab-count">{fileCount}</span>}
          </button>
        </nav>
      </DetailHeader>

      <div className={`detail-body${tab === "files" ? " pr-files-body" : ""}`}>
        {tab === "overview" ? (
          <Overview detail={detail} threads={threads} checks={checks} writes={writes} threadActions={canComment ? threadActions : undefined} />
        ) : (
          <FilesTab
            pr={pullRequest}
            files={files}
            threads={threads.data ?? NO_THREADS}
            commenting={canComment ? lineCommenting : undefined}
            readOnly={readOnly}
          />
        )}
      </div>
    </div>
  );
}
