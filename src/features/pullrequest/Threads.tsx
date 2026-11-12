// Pull request discussion threads, with reply and resolve when the detail can write.
import { useState } from "react";
import { Avatar, Badge, CommentCard } from "../../components/ui";
import { timeAgo } from "../../lib/format";
import { MAX_COMMENT_CHARS, type PullRequestThread, type ThreadComment, type ThreadStatusChange } from "./api";
import { usePullRequestWrite } from "./Actions";
import { threadStatus } from "./format";

/** Thread writes; absent when the detail is read-only or the pull request is closed. */
export interface ThreadActions {
  onReply: (thread: PullRequestThread, parentCommentId: number, content: string) => Promise<void>;
  onStatus: (thread: PullRequestThread, status: ThreadStatusChange) => Promise<void>;
  onChanged: () => void;
  onDenied: () => void;
}

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

/** Open threads can be resolved; resolved or closed ones reactivated. Unknown statuses get neither. */
function statusChange(status: string | null): { status: ThreadStatusChange; label: string } | null {
  switch (status) {
    case "active":
    case "pending":
      return { status: "fixed", label: "Resolve" };
    case "fixed":
    case "wontFix":
    case "closed":
    case "byDesign":
      return { status: "active", label: "Reactivate" };
    default:
      return null;
  }
}

/** Reply box and Resolve/Reactivate under a thread. The threads reload after every attempt. */
function ThreadFooter({ thread, actions }: { thread: PullRequestThread; actions: ThreadActions }) {
  const [replying, setReplying] = useState(false);
  const [text, setText] = useState("");
  const { busy, error, run } = usePullRequestWrite(actions.onChanged, actions.onDenied);
  const change = statusChange(thread.status);
  // Replies go under the thread's first comment, like the web.
  const root = thread.comments.find((c) => !c.isSystem && c.parentId === null);
  const tooLong = [...text.trim()].length > MAX_COMMENT_CHARS;

  const reply = async () => {
    if (!root || busy || text.trim() === "" || tooLong) return;
    if (await run(() => actions.onReply(thread, root.id, text))) {
      setText("");
      setReplying(false);
    }
  };

  if (!root && !change) return null;
  return (
    <div className="pr-thread-footer">
      {replying ? (
        <div className="diff-comment-box pr-thread-reply">
          <textarea
            autoFocus
            value={text}
            disabled={busy}
            aria-label="Reply"
            placeholder="Write a reply (Markdown)…"
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) void reply();
            }}
          />
          <div className="diff-comment-footer">
            <button className="btn btn-sm btn-primary" onClick={() => void reply()} disabled={busy || text.trim() === "" || tooLong}>
              {busy ? "Saving…" : "Reply"}
            </button>
            <button className="btn btn-sm btn-ghost" onClick={() => setReplying(false)} disabled={busy}>
              Cancel
            </button>
            {tooLong && <span className="pr-action-error">At most {MAX_COMMENT_CHARS} characters.</span>}
          </div>
        </div>
      ) : (
        <div className="pr-thread-buttons">
          {root && (
            <button className="btn btn-sm btn-ghost" onClick={() => setReplying(true)} disabled={busy}>
              Reply
            </button>
          )}
          {change && (
            <button className="btn btn-sm btn-ghost" onClick={() => void run(() => actions.onStatus(thread, change.status))} disabled={busy}>
              {busy ? "Saving…" : change.label}
            </button>
          )}
        </div>
      )}
      {error && (
        <p className="pr-action-error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

/** A discussion thread card. `showFile` adds the file/line header used in the Overview. */
export function ThreadCard({
  thread,
  showFile = false,
  actions,
}: {
  thread: PullRequestThread;
  showFile?: boolean;
  actions?: ThreadActions;
}) {
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
      {actions && <ThreadFooter thread={thread} actions={actions} />}
    </section>
  );
}

/** Discussion in time order: user threads as cards, system activity as compact lines. */
export function Discussion({ threads, actions }: { threads: PullRequestThread[]; actions?: ThreadActions }) {
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
          <ThreadCard key={thread.id} thread={thread} showFile={thread.kind === "file"} actions={actions} />
        ),
      )}
    </div>
  );
}
