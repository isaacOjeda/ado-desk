import { useCallback } from "react";
import type { WorkItem } from "../../lib/api";
import { useResource } from "../../lib/useResource";
import { workItemTypeTone } from "../../lib/format";
import { DetailHeader, Dot, ErrorBanner, OpenInBrowser } from "../../components/ui";
import { workItemApi } from "./api";
import { Discussion, LoadingBody, NotFoundBody, WorkItemForm, WorkItemMeta } from "./WorkItemSections";
import { CommentBox, OwnComment, ReadOnlyHint, WorkItemActionsBar, useWriteAccess, type Me } from "./WorkItemActions";
import "./workitem.css";

export interface WorkItemDetailViewProps {
  /** Row the user clicked; use it to render the header instantly while details load. */
  workItem: WorkItem;
  onBack: () => void;
  /** Signed-in user, offered as an assignee when Azure DevOps exposes their sign-in address. */
  me: Me | null;
}

/** Container: loads the work item and its comments, then hands them to presentational sections. */
export function WorkItemDetailView({ workItem, onBack, me }: WorkItemDetailViewProps) {
  const { id, project } = workItem;
  const fetchDetail = useCallback(() => workItemApi.get(id), [id]);
  const fetchComments = useCallback(() => workItemApi.comments(project, id), [project, id]);
  const detail = useResource(fetchDetail);
  const comments = useResource(fetchComments);

  const item = detail.data;
  const type = item?.workItemType ?? workItem.workItemType;
  const { canWrite, deny } = useWriteAccess(item);
  // After a write, read the item and its comments again (no optimistic updates).
  const reloadAll = () => {
    detail.reload();
    comments.reload();
  };

  return (
    <div className="detail">
      <DetailHeader
        onBack={onBack}
        eyebrow={
          <>
            <Dot tone={workItemTypeTone(type)} />
            {type} {workItem.id}
          </>
        }
        title={item?.title ?? workItem.title}
        actions={<OpenInBrowser url={item?.url ?? workItem.url} />}
      >
        <WorkItemMeta
          state={item?.state ?? workItem.state}
          project={item?.project || workItem.project}
          assignedTo={item ? item.assignedTo : undefined}
          changedAt={item?.changedAt ?? workItem.changedAt}
          changedBy={item?.changedBy}
        />
        {item && canWrite === true && (
          <WorkItemActionsBar item={item} me={me} onDenied={deny} onChanged={reloadAll} />
        )}
        {item && canWrite === false && <ReadOnlyHint />}
      </DetailHeader>

      <div className="detail-body">
        {detail.error && !item ? (
          detail.error.kind === "notFound" ? (
            <NotFoundBody />
          ) : (
            <div className="wi-error">
              <ErrorBanner message={detail.error.message} onRetry={detail.reload} />
            </div>
          )
        ) : !item ? (
          <LoadingBody />
        ) : (
          <WorkItemForm
            item={item}
            discussion={
              <Discussion
                comments={comments.data}
                loading={comments.loading}
                error={comments.error?.message ?? null}
                fallbackCount={item.commentCount}
                onRetry={comments.reload}
                composer={
                  canWrite === true ? <CommentBox item={item} onChanged={reloadAll} /> : undefined
                }
                renderComment={
                  canWrite === true
                    ? (comment, edited) =>
                        comment.isMine ? (
                          <OwnComment
                            item={item}
                            comment={comment}
                            edited={edited}
                            onChanged={reloadAll}
                          />
                        ) : null
                    : undefined
                }
              />
            }
          />
        )}
      </div>
    </div>
  );
}
