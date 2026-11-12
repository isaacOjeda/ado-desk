// Pull request write actions (vote, line comments). Every write runs in Rust; the detail is
// read again afterwards, whether the write succeeded or not (no optimistic updates).
import { useState } from "react";
import { ApiError } from "../../lib/api";
import { reviewerVote } from "./format";

export function errorText(err: unknown): string {
  return err instanceof ApiError ? err.message : "Something went wrong. Try again.";
}

/**
 * Runs one write, then `onChanged`. A refusal for lack of access calls `onDenied` (the detail
 * switches to read-only); other failures show inline so typed text is kept.
 */
export function usePullRequestWrite(onChanged: () => void, onDenied: () => void) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (write: () => Promise<void>): Promise<boolean> => {
    setBusy(true);
    setError(null);
    try {
      await write();
      return true;
    } catch (err) {
      if (err instanceof ApiError && err.kind === "writeDenied") onDenied();
      else setError(errorText(err));
      return false;
    } finally {
      setBusy(false);
      onChanged();
    }
  };

  return { busy, error, run };
}

export function PullRequestReadOnlyHint() {
  return (
    <p className="pr-readonly">
      Read-only: this token cannot change pull requests. Add Code (Read &amp; Write) to it to vote and comment.
    </p>
  );
}

const VOTE_CHOICES = [10, 5, -5, -10, 0] as const;

function voteChoiceLabel(vote: number): string {
  return vote === 0 ? "Reset vote" : reviewerVote(vote, false).label;
}

/** The signed-in user's vote. Changing it asks for confirmation first. */
export function VoteControl({
  myVote,
  onVote,
  onChanged,
  onDenied,
}: {
  myVote: number;
  onVote: (vote: number) => Promise<void>;
  onChanged: () => void;
  onDenied: () => void;
}) {
  const [pending, setPending] = useState<number | null>(null);
  const { busy, error, run } = usePullRequestWrite(onChanged, onDenied);
  const current = reviewerVote(myVote, false);

  const confirm = async () => {
    if (pending === null) return;
    const vote = pending;
    if (await run(() => onVote(vote))) setPending(null);
  };

  return (
    <div className="pr-vote">
      <p className="pr-vote-current">
        Your vote: <strong>{current.label}</strong>
      </p>
      {pending === null ? (
        <div className="pr-vote-choices" role="group" aria-label="Vote">
          {VOTE_CHOICES.filter((v) => v !== myVote).map((vote) => (
            <button key={vote} className={`btn btn-sm pr-vote-${vote < 0 ? "neg" : vote > 0 ? "pos" : "reset"}`} onClick={() => setPending(vote)}>
              {voteChoiceLabel(vote)}
            </button>
          ))}
        </div>
      ) : (
        <div className="pr-confirm">
          <span>{pending === 0 ? "Reset your vote?" : `Vote "${voteChoiceLabel(pending)}"?`}</span>
          <button className="btn btn-sm btn-primary" onClick={() => void confirm()} disabled={busy}>
            {busy ? "Saving…" : "Confirm"}
          </button>
          <button className="btn btn-sm btn-ghost" onClick={() => setPending(null)} disabled={busy}>
            Cancel
          </button>
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
