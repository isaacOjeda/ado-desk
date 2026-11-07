// Presentational pieces for pull request discussion threads.
import { Avatar, Badge, CommentCard } from "../../components/ui";
import { timeAgo } from "../../lib/format";
import type { PullRequestThread, ThreadComment } from "./api";
import { threadStatus } from "./format";

/** One line of Azure DevOps activity (votes, pushes, status changes). Plain text only. */
export function ActivityLine({ comment }: { comment: ThreadComment }) {
  return (
    <div className="pr-activity">
      <Avatar name={comment.author} size={20} />
      <span className="pr-activity-text">{comment.text}</span>
      {comment.publishedAt && (
        <time
          className="pr-activity-date"
          dateTime={comment.publishedAt}
          title={new Date(comment.publishedAt).toLocaleString()}
        >
          {timeAgo(comment.publishedAt)}
        </time>
      )}
    </div>
  );
}

function ThreadComments({ comments }: { comments: ThreadComment[] }) {
  return (
    <div className="pr-thread-comments">
      {comments.map((comment) =>
        comment.isSystem ? (
          <ActivityLine key={comment.id} comment={comment} />
        ) : (
          <div key={comment.id} className={comment.parentId ? "pr-reply" : undefined}>
            <CommentCard author={comment.author} date={comment.publishedAt} html={comment.html} />
          </div>
        ),
      )}
    </div>
  );
}

function lineLabel(thread: PullRequestThread): string {
  if (thread.rightLine) return `line ${thread.rightLine}`;
  if (thread.leftLine) return `line ${thread.leftLine} (original)`;
  return "file";
}

/** A discussion thread card. `showFile` adds the file/line header used in the Overview. */
export function ThreadCard({ thread, showFile = false }: { thread: PullRequestThread; showFile?: boolean }) {
  const status = threadStatus(thread.status);
  const showHeader = status || (showFile && thread.filePath);
  return (
    <section className="pr-thread">
      {showHeader && (
        <header className="pr-thread-header">
          {showFile && thread.filePath && (
            <span className="pr-thread-file" title={thread.filePath}>
              <span className="pr-mono">{thread.filePath}</span>
              <span className="pr-thread-line">{lineLabel(thread)}</span>
            </span>
          )}
          {status && <Badge tone={status.tone}>{status.label}</Badge>}
        </header>
      )}
      <ThreadComments comments={thread.comments} />
    </section>
  );
}

/** Discussion in time order: user threads as cards, system activity as compact lines. */
export function Discussion({ threads }: { threads: PullRequestThread[] }) {
  if (threads.length === 0) return <p className="rich-empty">No comments yet.</p>;
  return (
    <div className="pr-discussion">
      {threads.map((thread) =>
        thread.kind === "system" ? (
          <div key={thread.id} className="pr-activity-group">
            {thread.comments.map((comment) => (
              <ActivityLine key={comment.id} comment={comment} />
            ))}
          </div>
        ) : (
          <ThreadCard key={thread.id} thread={thread} showFile={thread.kind === "file"} />
        ),
      )}
    </div>
  );
}
