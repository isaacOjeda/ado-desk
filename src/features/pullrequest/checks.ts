// Build and policy status of pull requests; mirrors `src-tauri/src/checks.rs`.
import { useEffect, useState } from "react";
import { call } from "../../lib/api";
import type { Tone } from "../../lib/format";

export type CheckState = "succeeded" | "failed" | "running" | "queued" | "partial" | "canceled";
export type CheckSummary = "succeeded" | "failed" | "running" | "none";

export interface PolicyCheck {
  name: string;
  kind: "build" | "status" | "other";
  blocking: boolean;
  state: CheckState;
  /** The build run, for build validation policies that have one (built in Rust). */
  url: string | null;
}

export interface BuildCheck {
  id: number;
  definition: string;
  number: string;
  state: CheckState;
  source: "pullRequest" | "branch";
  finishedAt: string | null;
  url: string;
}

/** `null` halves are unavailable (missing scope or failed call), never an error. */
export interface PullRequestChecks {
  policies: PolicyCheck[] | null;
  builds: BuildCheck[] | null;
  summary: CheckSummary;
}

export interface CheckTarget {
  id: number;
  project: string;
  repositoryId: string;
}

interface CheckSummaryItem {
  id: number;
  repositoryId: string;
  summary: CheckSummary;
}

export const checksApi = {
  detail: ({ project, repositoryId, id }: CheckTarget) =>
    call<PullRequestChecks>("get_pull_request_checks", { project, repositoryId, id }),
  summaries: (items: CheckTarget[]) => call<CheckSummaryItem[]>("pull_request_check_summaries", { items }),
};

/** Rust reads at most this many pull requests per summaries call. */
const MAX_SUMMARIES = 50;

export const checkKey = (repositoryId: string, id: number) => `${repositoryId}:${id}`;

export function checkStateBadge(state: CheckState): { label: string; tone: Tone } {
  switch (state) {
    case "succeeded":
      return { label: "Succeeded", tone: "green" };
    case "failed":
      return { label: "Failed", tone: "red" };
    case "running":
      return { label: "Running", tone: "blue" };
    case "queued":
      return { label: "Queued", tone: "neutral" };
    case "partial":
      return { label: "Partially succeeded", tone: "amber" };
    case "canceled":
      return { label: "Canceled", tone: "neutral" };
  }
}

/** The list dot; `null` when there is nothing to show. */
export function summaryDot(summary: CheckSummary | undefined): { label: string; tone: Tone } | null {
  switch (summary) {
    case "succeeded":
      return { label: "Checks succeeded", tone: "green" };
    case "failed":
      return { label: "Checks failed", tone: "red" };
    case "running":
      return { label: "Checks running", tone: "amber" };
    default:
      return null;
  }
}

/**
 * List dots of the given pull requests, keyed by `checkKey`. Loaded after the list renders and
 * refetched only when the organization or the set of pull requests changes. Failures leave the
 * list without dots.
 */
export function useCheckSummaries(organization: string, targets: CheckTarget[] | null): Map<string, CheckSummary> {
  const [summaries, setSummaries] = useState<Map<string, CheckSummary>>(() => new Map());
  const limited = (targets ?? []).slice(0, MAX_SUMMARIES);
  const key = `${organization}|${limited.map((t) => checkKey(t.repositoryId, t.id)).join(",")}`;

  useEffect(() => {
    if (limited.length === 0) {
      setSummaries(new Map());
      return;
    }
    let active = true;
    checksApi
      .summaries(limited)
      .then((items) => {
        if (active) setSummaries(new Map(items.map((i) => [checkKey(i.repositoryId, i.id), i.summary])));
      })
      .catch(() => active && setSummaries(new Map()));
    return () => {
      active = false;
    };
    // `key` captures organization and the target list; `limited` is rebuilt every render.
  }, [key]);

  return summaries;
}
