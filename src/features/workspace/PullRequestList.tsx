import type { PullRequest } from "../../lib/api";
import { timeAgo, voteLabel } from "../../lib/format";
import { Badge, EmptyState } from "../../components/ui";

interface Props {
  pullRequests: PullRequest[];
  onSelect: (pullRequest: PullRequest) => void;
}

export function PullRequestList({ pullRequests, onSelect }: Props) {
  if (pullRequests.length === 0) {
    return <EmptyState title="No active pull requests" hint="PRs you created or review show up here." />;
  }

  return (
    <ul className="list">
      {pullRequests.map((pr) => {
        const vote = voteLabel(pr.role === "reviewer" ? pr.myVote : null);
        return (
          <li key={pr.id}>
            <button className="row" onClick={() => onSelect(pr)}>
              <span className={`pr-icon ${pr.isDraft ? "is-draft" : ""}`} aria-hidden>
                ⇄
              </span>
              <span className="row-id">!{pr.id}</span>
              <span className="row-main">
                <span className="row-title">
                  {pr.isDraft && <Badge>Draft</Badge>} {pr.title}
                </span>
                <span className="row-meta">
                  {pr.repository} · <code>{pr.sourceBranch}</code> → <code>{pr.targetBranch}</code> ·{" "}
                  {pr.author}
                </span>
              </span>
              {vote.label && <Badge tone={vote.tone}>{vote.label}</Badge>}
              <span className="approvals" title="Approvals">
                ✓ {pr.approvals}/{pr.reviewerCount}
              </span>
              <span className="row-time">{timeAgo(pr.createdAt)}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
