// Container for the read-only pull request detail: loads data and owns tab state.
// Layout mirrors the Azure DevOps web pull request page (Overview / Files).
import { useCallback, useState } from "react";
import { Badge, DetailHeader, OpenInBrowser } from "../../components/ui";
import type { PullRequest } from "../../lib/api";
import { timeAgo } from "../../lib/format";
import { useResource } from "../../lib/useResource";
import { pullRequestApi, type PullRequestThread } from "./api";
import { checksApi } from "./checks";
import { FilesTab } from "./Files";
import { statusBadge } from "./format";
import { Overview } from "./Overview";
import "./pullrequest.css";

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
        actions={<OpenInBrowser url={pullRequest.url} />}
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
          <Overview detail={detail} threads={threads} checks={checks} />
        ) : (
          <FilesTab pr={pullRequest} files={files} threads={threads.data ?? NO_THREADS} />
        )}
      </div>
    </div>
  );
}
