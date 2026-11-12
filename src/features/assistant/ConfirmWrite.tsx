// Confirmation for writes the assistant's session asks for (reply to a thread, comment on a work
// item). Nothing is posted until the user approves the exact text here; Esc or Cancel declines.
import { useEffect, useRef, useState } from "react";
import { subscribe } from "../../lib/api";
import { assistantApi, type WriteRequest } from "./api";

const POST = "post";
/** Post stays disabled this long after a request appears, so a click meant for the previous
 * request (or for the page behind) cannot post a new one. */
const ARM_DELAY_MS = 700;

export function ConfirmWrite() {
  const [queue, setQueue] = useState<WriteRequest[]>([]);
  const dialogRef = useRef<HTMLDialogElement>(null);
  const current = queue[0] ?? null;
  const [armed, setArmed] = useState(false);
  const currentId = current?.id;

  // Every new request, including one that replaces an expired request in an open dialog,
  // starts disarmed.
  useEffect(() => {
    setArmed(false);
    if (currentId === undefined) return;
    const timer = window.setTimeout(() => setArmed(true), ARM_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [currentId]);

  useEffect(
    () =>
      subscribe(() =>
        // A request is queued once, even if an event is delivered twice.
        assistantApi.onConfirmRequested((request) =>
          setQueue((q) => (q.some((r) => r.id === request.id) ? q : [...q, request])),
        ),
      ),
    [],
  );
  useEffect(
    () => subscribe(() => assistantApi.onConfirmClosed((id) => setQueue((q) => q.filter((r) => r.id !== id)))),
    [],
  );

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    if (current && !dialog.open) dialog.showModal();
    if (!current && dialog.open) dialog.close();
  }, [current]);

  const onClose = () => {
    const dialog = dialogRef.current;
    if (!dialog || !current) return;
    const approved = dialog.returnValue === POST;
    // Esc sets no value and browsers differ on clearing the previous one.
    dialog.returnValue = "";
    assistantApi.confirm(current.id, approved).catch(() => {});
    setQueue((q) => q.filter((r) => r.id !== current.id));
  };

  return (
    <dialog ref={dialogRef} className="assistant-confirm" aria-labelledby="assistant-confirm-title" onClose={onClose}>
      {current && (
        // Keyed by request: a replaced request remounts, so focus returns to Cancel.
        <form method="dialog" key={current.id}>
          <h2 id="assistant-confirm-title">The assistant wants to post as you</h2>
          <dl className="assistant-confirm-facts">
            <dt>Action</dt>
            <dd>{current.action}</dd>
            <dt>Where</dt>
            <dd>
              {current.target} · {current.organization}
            </dd>
          </dl>
          <pre className="assistant-confirm-text">{current.text}</pre>
          <p className="assistant-confirm-hint">
            It is posted under your name and others will see it. Check the text: it may quote content written by others.
          </p>
          <div className="assistant-confirm-buttons">
            {queue.length > 1 && <span className="assistant-confirm-queue">{queue.length - 1} more waiting</span>}
            <button className="btn btn-ghost btn-sm" value="cancel" autoFocus>
              Cancel
            </button>
            <button className="btn btn-primary btn-sm" value={POST} disabled={!armed}>
              Post
            </button>
          </div>
        </form>
      )}
    </dialog>
  );
}
