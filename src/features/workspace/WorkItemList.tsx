import type { WorkItem } from "../../lib/api";
import { stateTone, timeAgo, workItemTypeTone } from "../../lib/format";
import { Badge, Dot, EmptyState } from "../../components/ui";

interface Props {
  items: WorkItem[];
  onSelect: (item: WorkItem) => void;
  /** True when filters other than the defaults are applied. */
  isFiltered?: boolean;
}

export function WorkItemList({ items, onSelect, isFiltered = false }: Props) {
  if (items.length === 0) {
    return isFiltered ? (
      <EmptyState title="No work items match these filters" hint="Change or reset the filters above." />
    ) : (
      <EmptyState title="Nothing assigned to you" hint="Open work items assigned to you show up here." />
    );
  }

  return (
    <ul className="list">
      {items.map((item) => (
        <li key={item.id}>
          <button className="row" onClick={() => onSelect(item)}>
            <Dot tone={workItemTypeTone(item.workItemType)} />
            <span className="row-id">{item.id}</span>
            <span className="row-main">
              <span className="row-title">{item.title}</span>
              <span className="row-meta">
                {item.workItemType} · {item.project}
                {item.priority != null && ` · P${item.priority}`}
              </span>
            </span>
            <Badge tone={stateTone(item.state)}>{item.state}</Badge>
            <span className="row-time">{timeAgo(item.changedAt)}</span>
          </button>
        </li>
      ))}
    </ul>
  );
}
