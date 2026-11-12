// Overview tab: description and discussion, with reviewers and links in the side panel.
import { openUrl } from "@tauri-apps/plugin-opener";
import { Avatar, Badge, RichText } from "../../components/ui";
import { workItemTypeTone, stateTone } from "../../lib/format";
import type { Resource } from "../../lib/useResource";
import type { LineSelection, PullRequestDetail, PullRequestThread, Reviewer } from "./api";
import type { PullRequestChecks } from "./checks";
import { ChecksSection } from "./ChecksSection";
import { Async } from "./Async";
import { mergeStatus, reviewerVote } from "./format";
import { Discussion, type ThreadActions } from "./Threads";
import { PullRequestReadOnlyHint, VoteControl } from "./Actions";

/** Write hooks of the detail; absent pieces render read-only. */
export interface PullRequestWrites {
  readOnly: boolean;
  onVote: (vote: number) => Promise<void>;
  onLineComment: (selection: LineSelection, content: string) => Promise<void>;
  onChanged: () => void;
  onDenied: () => void;
}

function ReviewerRow({ reviewer }: { reviewer: Reviewer }) {
  const vote = reviewerVote(reviewer.vote, reviewer.hasDeclined);
  return (
    <li className="pr-reviewer">
      <Avatar name={reviewer.name} size={24} />
      <span className="pr-reviewer-main">
        <span className="pr-reviewer-name" title={reviewer.name}>
          {reviewer.name}
        </span>
        <span className="pr-reviewer-role">{reviewer.isRequired ? "Required" : "Optional"}</span>
      </span>
      <Badge tone={vote.tone}>{vote.label}</Badge>
    </li>
  );
}

function Aside({
  detail,
  checks,
  writes,
}: {
  detail: PullRequestDetail;
  checks: Resource<PullRequestChecks>;
  writes: PullRequestWrites;
}) {
  const merge = mergeStatus(detail.mergeStatus);
  const open = detail.status === "active" || detail.status === "draft";
  return (
    <aside className="detail-aside">
      {open && (
        <section>
          <h3 className="section-title">Vote</h3>
          {writes.readOnly ? (
            <PullRequestReadOnlyHint />
          ) : (
            <VoteControl myVote={detail.myVote} onVote={writes.onVote} onChanged={writes.onChanged} onDenied={writes.onDenied} />
          )}
        </section>
      )}

      <ChecksSection checks={checks} />

      <section>
        <h3 className="section-title">Reviewers</h3>
        {detail.reviewers.length === 0 ? (
          <p className="rich-empty">No reviewers</p>
        ) : (
          <ul className="pr-plain-list">
            {detail.reviewers.map((r) => (
              <ReviewerRow key={r.id} reviewer={r} />
            ))}
          </ul>
        )}
      </section>

      <section>
        <h3 className="section-title">Work items</h3>
        {detail.workItems.length === 0 ? (
          <p className="rich-empty">No linked work items</p>
        ) : (
          <ul className="pr-plain-list">
            {detail.workItems.map((item) => (
              <li key={item.id}>
                <button className="pr-work-item" onClick={() => void openUrl(item.url)} title="Open in browser">
                  <Badge tone={workItemTypeTone(item.workItemType)}>{item.workItemType}</Badge>
                  <span className="pr-work-item-id">{item.id}</span>
                  <span className="pr-work-item-title">{item.title}</span>
                  <Badge tone={stateTone(item.state)}>{item.state}</Badge>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      {detail.labels.length > 0 && (
        <section>
          <h3 className="section-title">Labels</h3>
          <div className="pr-labels">
            {detail.labels.map((label) => (
              <Badge key={label}>{label}</Badge>
            ))}
          </div>
        </section>
      )}

      {merge && (
        <section>
          <h3 className="section-title">Merge</h3>
          <Badge tone={merge.tone}>{merge.label}</Badge>
        </section>
      )}
    </aside>
  );
}

export function Overview({
  detail,
  threads,
  checks,
  writes,
  threadActions,
}: {
  detail: Resource<PullRequestDetail>;
  threads: Resource<PullRequestThread[]>;
  checks: Resource<PullRequestChecks>;
  writes: PullRequestWrites;
  /** Reply and resolve; absent when read-only or the pull request is closed. */
  threadActions?: ThreadActions;
}) {
  return (
    <Async resource={detail}>
      {(pr) => (
        <div className="detail-layout">
          <div>
            <section className="section">
              <h3 className="section-title">Description</h3>
              {pr.descriptionHtml ? (
                <RichText html={pr.descriptionHtml} />
              ) : (
                <p className="rich-empty">No description provided.</p>
              )}
            </section>
            <section className="section">
              <h3 className="section-title">Discussion</h3>
              <Async resource={threads}>{(list) => <Discussion threads={list} actions={threadActions} />}</Async>
            </section>
          </div>
          <Aside detail={pr} checks={checks} writes={writes} />
        </div>
      )}
    </Async>
  );
}
