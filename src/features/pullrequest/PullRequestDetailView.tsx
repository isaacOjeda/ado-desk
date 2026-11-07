// STUB: owned by task T3 (odd/tasks/detail-views.md). Replace the body; keep the props contract.
import type { PullRequest } from "../../lib/api";
import { DetailHeader } from "../../components/ui";

export interface PullRequestDetailViewProps {
  /** Row the user clicked; use it to render the header instantly while details load. */
  pullRequest: PullRequest;
  onBack: () => void;
}

export function PullRequestDetailView({ pullRequest, onBack }: PullRequestDetailViewProps) {
  return <DetailHeader onBack={onBack} eyebrow={`Pull request !${pullRequest.id}`} title={pullRequest.title} />;
}
