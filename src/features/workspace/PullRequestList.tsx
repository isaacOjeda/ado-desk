import type { FilteredPullRequest } from "../filters/api";
import { pullRequestStatusBadge, timeAgo, voteLabel } from "../../lib/format";
import { Badge, Dot, EmptyState } from "../../components/ui";
import { checkKey, summaryDot, type CheckSummary } from "../pullrequest/checks";

interface Props {
  pullRequests: FilteredPullRequest[];
  onSelect: (pullRequest: FilteredPullRequest) => void;
  /** True when filters other than the defaults are applied. */
  isFiltered?: boolean;
  /** List dots by `checkKey`; pull requests without an entry show none. */
  checks?: Map<string, CheckSummary>;
}

export function PullRequestList({ pullRequests, onSelect, isFiltered = false, checks }: Props) {
  if (pullRequests.length === 0) {
    return isFiltered ? (
      <EmptyState title="No pull requests match these filters" hint="Change or reset the filters above." />
    ) : (
      <EmptyState title="No active pull requests" hint="PRs you created or review show up here." />
    );
  }

  return (
    <ul className="list">
      {pullRequests.map((pr) => {
        const closed = pullRequestStatusBadge(pr.status);
        const vote = closed ? { label: "", tone: "neutral" as const } : voteLabel(pr.role === "reviewer" ? pr.myVote : null);
        const time = closed ? (pr.closedAt ?? pr.createdAt) : pr.createdAt;
        const dot = closed ? null : summaryDot(checks?.get(checkKey(pr.repositoryId, pr.id)));
        return (
          <li key={pr.id}>
            <button className="row" onClick={() => onSelect(pr)}>
              <span className={`pr-icon ${pr.isDraft ? "is-draft" : ""}`} aria-hidden>
                ⇄
              </span>
              <span className="row-id">!{pr.id}</span>
              <span className="row-main">
                <span className="row-title">
                  {pr.isDraft && !closed && <Badge>Draft</Badge>} {pr.title}
                </span>
                <span className="row-meta">
                  {pr.repository} · <code>{pr.sourceBranch}</code> → <code>{pr.targetBranch}</code> ·{" "}
                  {pr.author}
                </span>
              </span>
              {closed && <Badge tone={closed.tone}>{closed.label}</Badge>}
              {vote.label && <Badge tone={vote.tone}>{vote.label}</Badge>}
              {dot && (
                <span className="check-dot" title={dot.label} role="img" aria-label={dot.label}>
                  <Dot tone={dot.tone} />
                </span>
              )}
              <span className="approvals" title="Approvals">
                ✓ {pr.approvals}/{pr.reviewerCount}
              </span>
              <span className="row-time" title={closed ? `${closed.label} ${timeAgo(time)}` : undefined}>
                {timeAgo(time)}
              </span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
