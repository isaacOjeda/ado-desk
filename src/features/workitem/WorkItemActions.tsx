// Work item actions (roadmap phase 6): change the state, change the assignee, add a comment.
// Every write runs in Rust; afterwards the detail reloads from Azure DevOps (no optimistic
// updates). Failures stay next to the action and never sign the user out.
import { useEffect, useState, type FormEvent } from "react";
import { ApiError } from "../../lib/api";
import { filtersApi, type TeamMember } from "../filters/api";
import { MAX_COMMENT_CHARS, workItemActionsApi, type WorkItemDetail } from "./api";

export interface Me {
  displayName: string;
  /** Unique name Azure DevOps accepts as an assignee. */
  email: string;
}

const UNASSIGN = "\u0000unassign";

function errorText(err: unknown): string {
  return err instanceof ApiError ? err.message : String(err);
}

/**
 * Whether the token can change work items: `null` until known. Probed once per detail (Rust
 * remembers the answer for the organization); `deny` switches to read-only after a write was
 * refused for lack of access.
 */
export function useWriteAccess(item: WorkItemDetail | null): { canWrite: boolean | null; deny: () => void } {
  const [canWrite, setCanWrite] = useState<boolean | null>(null);
  const id = item?.id;
  const rev = item?.rev;

  useEffect(() => {
    if (id === undefined || rev === undefined || canWrite !== null) return;
    let active = true;
    workItemActionsApi
      .writeAccess(id, rev)
      .then((allowed) => active && setCanWrite(allowed))
      // An unclear answer shows the actions; a refused write then hides them.
      .catch(() => active && setCanWrite(true));
    return () => {
      active = false;
    };
  }, [id, rev, canWrite]);

  return { canWrite, deny: () => setCanWrite(false) };
}

/** Runs one write: reports a refusal for lack of access, reloads the item after success. */
function useWrite(onDenied: () => void, onChanged: () => void) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (write: () => Promise<void>): Promise<boolean> => {
    setBusy(true);
    setError(null);
    try {
      await write();
      onChanged();
      return true;
    } catch (err) {
      if (err instanceof ApiError && err.kind === "writeDenied") onDenied();
      else setError(errorText(err));
      return false;
    } finally {
      setBusy(false);
    }
  };

  return { busy, error, run };
}

export function ReadOnlyHint() {
  return (
    <p className="wi-readonly">
      Read-only: this token cannot change work items. Add Work Items (Read &amp; Write) to it to enable actions.
    </p>
  );
}

/** State and assignee pickers under the header. A state change asks for confirmation first. */
export function WorkItemActionsBar({
  item,
  me,
  onDenied,
  onChanged,
}: {
  item: WorkItemDetail;
  me: Me | null;
  onDenied: () => void;
  onChanged: () => void;
}) {
  const [states, setStates] = useState<string[] | null>(null);
  const [members, setMembers] = useState<TeamMember[] | null>(null);
  const [pendingState, setPendingState] = useState<string | null>(null);
  const { busy, error, run } = useWrite(onDenied, onChanged);
  const { project, workItemType } = item;

  useEffect(() => {
    let active = true;
    workItemActionsApi
      .states(project, workItemType)
      .then((list) => active && setStates(list))
      .catch(() => active && setStates([]));
    filtersApi
      .listTeamMembers(project)
      .then((list) => active && setMembers(list))
      .catch(() => active && setMembers([]));
    return () => {
      active = false;
    };
  }, [project, workItemType]);

  const stateOptions = states && states.includes(item.state) ? states : [item.state, ...(states ?? [])];
  const assignees = (members ?? []).filter(
    (m) => m.displayName !== item.assignedTo && m.uniqueName.toLowerCase() !== me?.email.toLowerCase(),
  );

  const changeAssignee = (value: string) => {
    if (!value) return;
    void run(() => workItemActionsApi.setAssignee(item, value === UNASSIGN ? null : value));
  };

  return (
    <div className="wi-actions">
      {pendingState !== null ? (
        <div className="wi-confirm" role="group" aria-label="Confirm the state change">
          <span>
            Change state to <strong>{pendingState}</strong>?
          </span>
          <button className="btn btn-ghost btn-sm" onClick={() => setPendingState(null)} disabled={busy}>
            Cancel
          </button>
          <button
            className="btn btn-primary btn-sm"
            disabled={busy}
            onClick={async () => {
              const value = pendingState;
              if (await run(() => workItemActionsApi.setState(item, value))) setPendingState(null);
            }}
          >
            Change state
          </button>
        </div>
      ) : (
        <>
          <label className="wi-action">
            <span>State</span>
            <select
              value={item.state}
              disabled={busy || states === null}
              title={states?.length === 0 ? "States unavailable" : undefined}
              onChange={(e) => e.target.value !== item.state && setPendingState(e.target.value)}
            >
              {stateOptions.map((state) => (
                <option key={state} value={state}>
                  {state}
                </option>
              ))}
            </select>
          </label>
          <label className="wi-action">
            <span>Assign to</span>
            <select value="" disabled={busy || members === null} onChange={(e) => changeAssignee(e.target.value)}>
              <option value="" disabled hidden>
                {item.assignedTo ?? "Unassigned"}
              </option>
              {item.assignedTo && <option value={UNASSIGN}>Unassigned</option>}
              {me && me.displayName !== item.assignedTo && <option value={me.email}>Me ({me.displayName})</option>}
              {assignees.map((m) => (
                <option key={m.uniqueName} value={m.uniqueName}>
                  {m.displayName}
                </option>
              ))}
            </select>
          </label>
        </>
      )}
      {error && (
        <p className="wi-action-error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

/** New comment, above the discussion. Plain text: Rust escapes it before sending. */
export function CommentBox({
  item,
  onDenied,
  onChanged,
}: {
  item: WorkItemDetail;
  onDenied: () => void;
  onChanged: () => void;
}) {
  const [text, setText] = useState("");
  const { busy, error, run } = useWrite(onDenied, onChanged);
  // Code points, as Rust counts them.
  const length = [...text.trim()].length;
  const tooLong = length > MAX_COMMENT_CHARS;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (length === 0 || tooLong) return;
    if (await run(() => workItemActionsApi.addComment(item, text))) setText("");
  };

  return (
    <form className="wi-comment-box" onSubmit={submit}>
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        placeholder="Add a comment"
        rows={3}
        disabled={busy}
        aria-label="New comment"
      />
      <div className="wi-comment-footer">
        <span className={`wi-comment-count ${tooLong ? "is-over" : ""}`}>
          {length}/{MAX_COMMENT_CHARS}
        </span>
        {error && (
          <span className="wi-action-error" role="alert">
            {error}
          </span>
        )}
        <button className="btn btn-primary btn-sm" type="submit" disabled={busy || length === 0 || tooLong}>
          {busy ? "Posting…" : "Comment"}
        </button>
      </div>
    </form>
  );
}
