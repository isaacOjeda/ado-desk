// Presentational pieces of the work item form. They only render props; fetching lives in
// `WorkItemDetailView`.
import type { ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Avatar, Badge, CommentCard, Dot, EmptyState, ErrorBanner, RichText, Spinner } from "../../components/ui";
import { stateTone, timeAgo, workItemTypeTone } from "../../lib/format";
import type { DevelopmentKind, DevelopmentLink, LinkedWorkItem, WorkItemComment, WorkItemDetail } from "./api";

function formatDate(iso: string | null): string {
  return iso ? new Date(iso).toLocaleString() : "";
}

function formatNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(2).replace(/0+$/, "");
}

/** Meta row under the title: state, assignee, project and last update. */
export function WorkItemMeta({
  state,
  project,
  assignedTo,
  changedAt,
  changedBy,
}: {
  state: string;
  project: string;
  assignedTo: string | null | undefined;
  changedAt: string | null;
  changedBy: string | null | undefined;
}) {
  return (
    <div className="detail-meta">
      <Badge tone={stateTone(state)}>{state}</Badge>
      {assignedTo !== undefined && (
        <span className="wi-person">
          {assignedTo ? (
            <>
              <Avatar name={assignedTo} size={20} />
              {assignedTo}
            </>
          ) : (
            <span className="wi-unassigned">Unassigned</span>
          )}
        </span>
      )}
      <span>{project}</span>
      {changedAt && (
        <span title={formatDate(changedAt)}>
          Updated {timeAgo(changedAt)}
          {changedBy ? ` by ${changedBy}` : ""}
        </span>
      )}
    </div>
  );
}

/** A long text field (Description, Repro Steps, ...). `html` must be sanitized in Rust. */
export function RichSection({ title, html, placeholder }: { title: string; html: string | null; placeholder: string }) {
  return (
    <section className="section">
      <h2 className="section-title">{title}</h2>
      {html ? <RichText html={html} /> : <p className="rich-empty">{placeholder}</p>}
    </section>
  );
}

export function Discussion({
  comments,
  loading,
  error,
  fallbackCount,
  onRetry,
  composer,
}: {
  comments: WorkItemComment[] | null;
  loading: boolean;
  error: string | null;
  fallbackCount: number | null;
  onRetry: () => void;
  /** New comment box, shown above the comments when the token can write. */
  composer?: ReactNode;
}) {
  const count = comments?.length ?? fallbackCount;
  return (
    <section className="section">
      <h2 className="section-title">Discussion{count !== null ? ` (${count})` : ""}</h2>
      {composer}
      {error ? (
        <ErrorBanner message={error} onRetry={onRetry} />
      ) : loading && !comments ? (
        <div className="wi-loading">
          <Spinner />
        </div>
      ) : comments && comments.length > 0 ? (
        comments.map((comment) => (
          <CommentCard
            key={comment.id}
            author={comment.author}
            date={comment.createdAt}
            html={comment.html}
            footer={
              comment.isEdited ? (
                <p className="wi-edited" title={formatDate(comment.modifiedAt)}>
                  (edited)
                </p>
              ) : undefined
            }
          />
        ))
      ) : (
        <p className="rich-empty">No comments yet.</p>
      )}
    </section>
  );
}

function Prop({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </>
  );
}

/** Right-hand "Details" panel, like the Azure DevOps form. */
export function DetailsPanel({ item }: { item: WorkItemDetail }) {
  const numbers: [string, number | null][] = [
    ["Story points", item.storyPoints],
    ["Effort", item.effort],
    ["Original est.", item.originalEstimate],
    ["Remaining", item.remainingWork],
    ["Completed", item.completedWork],
  ];
  return (
    <section className="section">
      <h2 className="section-title">Details</h2>
      <dl className="props">
        <Prop label="State">
          <Badge tone={stateTone(item.state)}>{item.state}</Badge>
        </Prop>
        {item.reason && <Prop label="Reason">{item.reason}</Prop>}
        <Prop label="Assigned to">
          {item.assignedTo ? (
            <span className="wi-person">
              <Avatar name={item.assignedTo} size={20} />
              {item.assignedTo}
            </span>
          ) : (
            <span className="wi-unassigned">Unassigned</span>
          )}
        </Prop>
        {item.priority !== null && <Prop label="Priority">{item.priority}</Prop>}
        {item.severity && <Prop label="Severity">{item.severity}</Prop>}
        {item.areaPath && <Prop label="Area">{item.areaPath}</Prop>}
        {item.iterationPath && <Prop label="Iteration">{item.iterationPath}</Prop>}
        {item.tags.length > 0 && (
          <Prop label="Tags">
            <span className="wi-tags">
              {item.tags.map((tag) => (
                <Badge key={tag}>{tag}</Badge>
              ))}
            </span>
          </Prop>
        )}
        {numbers.map(([label, value]) =>
          value !== null ? (
            <Prop key={label} label={label}>
              {formatNumber(value)}
            </Prop>
          ) : null,
        )}
        {item.createdBy && (
          <Prop label="Created by">
            {item.createdBy}
            {item.createdAt && <span className="wi-sub">{formatDate(item.createdAt)}</span>}
          </Prop>
        )}
        {item.changedAt && (
          <Prop label="Changed">
            <span title={formatDate(item.changedAt)}>{timeAgo(item.changedAt)}</span>
            {item.changedBy && <span className="wi-sub">by {item.changedBy}</span>}
          </Prop>
        )}
      </dl>
    </section>
  );
}

function LinkRow({ item }: { item: LinkedWorkItem }) {
  return (
    <li>
      <button className="wi-link" onClick={() => void openUrl(item.url)} title={`Open ${item.id} in the browser`}>
        {item.workItemType && <Dot tone={workItemTypeTone(item.workItemType)} />}
        <span className="wi-link-id">{item.id}</span>
        <span className="wi-link-title">{item.title}</span>
        {item.state && <Badge tone={stateTone(item.state)}>{item.state}</Badge>}
      </button>
    </li>
  );
}

function LinkGroup({ title, items }: { title: string; items: LinkedWorkItem[] }) {
  if (items.length === 0) return null;
  return (
    <div className="wi-link-group">
      <h3 className="wi-link-group-title">{title}</h3>
      <ul className="wi-links">
        {items.map((item) => (
          <LinkRow key={`${title}-${item.id}`} item={item} />
        ))}
      </ul>
    </div>
  );
}

/** Parent, children and other work item links, grouped by link type. */
export function RelatedWork({ item }: { item: WorkItemDetail }) {
  const byLink = new Map<string, LinkedWorkItem[]>();
  for (const link of item.related) byLink.set(link.link, [...(byLink.get(link.link) ?? []), link]);
  const empty = !item.parent && item.children.length === 0 && item.related.length === 0;

  return (
    <section className="section">
      <h2 className="section-title">Related work</h2>
      {empty ? (
        <p className="rich-empty">No related work items.</p>
      ) : (
        <>
          <LinkGroup title="Parent" items={item.parent ? [item.parent] : []} />
          <LinkGroup title="Children" items={item.children} />
          {[...byLink].map(([link, items]) => (
            <LinkGroup key={link} title={link} items={items} />
          ))}
        </>
      )}
    </section>
  );
}

const DEVELOPMENT_ICONS: Record<DevelopmentKind, string> = {
  pullRequest: "⇄",
  commit: "◉",
  branch: "⑂",
  build: "▶",
  hyperlink: "↗",
  other: "•",
};

function DevelopmentRow({ link }: { link: DevelopmentLink }) {
  const content = (
    <>
      <span className="wi-dev-icon" aria-hidden>
        {DEVELOPMENT_ICONS[link.kind]}
      </span>
      <span className="wi-link-title">{link.label}</span>
      {link.link !== link.label && <span className="wi-sub">{link.link}</span>}
    </>
  );
  return (
    <li>
      {link.url ? (
        <button className="wi-link" onClick={() => void openUrl(link.url!)} title="Open in the browser">
          {content}
        </button>
      ) : (
        <span className="wi-link is-static">{content}</span>
      )}
    </li>
  );
}

/** Pull requests, commits, branches, builds and hyperlinks linked to the work item. */
export function Development({ links }: { links: DevelopmentLink[] }) {
  return (
    <section className="section">
      <h2 className="section-title">Development</h2>
      {links.length === 0 ? (
        <p className="rich-empty">No linked development.</p>
      ) : (
        <ul className="wi-links">
          {links.map((link, index) => (
            <DevelopmentRow key={`${link.kind}-${link.url ?? link.label}-${index}`} link={link} />
          ))}
        </ul>
      )}
    </section>
  );
}

/** Full body once the work item is loaded: main column plus the properties aside. */
export function WorkItemForm({ item, discussion }: { item: WorkItemDetail; discussion: ReactNode }) {
  const isBug = item.workItemType.toLowerCase() === "bug";
  return (
    <div className="detail-layout">
      <div className="wi-main">
        <RichSection title="Description" html={item.descriptionHtml} placeholder="No description." />
        {(item.reproStepsHtml || isBug) && (
          <RichSection title="Repro Steps" html={item.reproStepsHtml} placeholder="No repro steps." />
        )}
        {item.systemInfoHtml && <RichSection title="System Info" html={item.systemInfoHtml} placeholder="" />}
        {item.acceptanceCriteriaHtml && (
          <RichSection title="Acceptance Criteria" html={item.acceptanceCriteriaHtml} placeholder="" />
        )}
        {discussion}
      </div>
      <aside className="detail-aside">
        <DetailsPanel item={item} />
        <RelatedWork item={item} />
        <Development links={item.development} />
      </aside>
    </div>
  );
}

export function LoadingBody() {
  return (
    <div className="wi-loading">
      <Spinner />
    </div>
  );
}

export function NotFoundBody() {
  return <EmptyState title="Work item not found" hint="It may have been deleted or you may lack access." />;
}
