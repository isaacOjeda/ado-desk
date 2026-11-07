// STUB: owned by task T2 (odd/tasks/detail-views.md). Replace the body; keep the props contract.
import type { WorkItem } from "../../lib/api";
import { DetailHeader } from "../../components/ui";

export interface WorkItemDetailViewProps {
  /** Row the user clicked; use it to render the header instantly while details load. */
  workItem: WorkItem;
  onBack: () => void;
}

export function WorkItemDetailView({ workItem, onBack }: WorkItemDetailViewProps) {
  return <DetailHeader onBack={onBack} eyebrow={`${workItem.workItemType} ${workItem.id}`} title={workItem.title} />;
}
