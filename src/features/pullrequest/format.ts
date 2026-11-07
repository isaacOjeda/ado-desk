import type { Tone } from "../../lib/format";
import type { FileChangeType, PullRequestStatus, PullRequestThread } from "./api";

export function statusBadge(status: PullRequestStatus): { label: string; tone: Tone } {
  switch (status) {
    case "draft":
      return { label: "Draft", tone: "neutral" };
    case "completed":
      return { label: "Completed", tone: "violet" };
    case "abandoned":
      return { label: "Abandoned", tone: "red" };
    default:
      return { label: "Active", tone: "green" };
  }
}

/** Vote of any reviewer (unlike `voteLabel`, which speaks to the signed-in user). */
export function reviewerVote(vote: number, hasDeclined: boolean): { label: string; tone: Tone } {
  if (hasDeclined) return { label: "Declined", tone: "neutral" };
  switch (vote) {
    case 10:
      return { label: "Approved", tone: "green" };
    case 5:
      return { label: "Approved w/ suggestions", tone: "green" };
    case -5:
      return { label: "Waiting for author", tone: "amber" };
    case -10:
      return { label: "Rejected", tone: "red" };
    default:
      return { label: "No vote", tone: "neutral" };
  }
}

export function threadStatus(status: string | null): { label: string; tone: Tone } | null {
  switch (status) {
    case "active":
      return { label: "Active", tone: "blue" };
    case "pending":
      return { label: "Pending", tone: "amber" };
    case "fixed":
      return { label: "Resolved", tone: "green" };
    case "wontFix":
      return { label: "Won't fix", tone: "neutral" };
    case "closed":
      return { label: "Closed", tone: "neutral" };
    case "byDesign":
      return { label: "By design", tone: "neutral" };
    default:
      return null;
  }
}

export function mergeStatus(status: string | null): { label: string; tone: Tone } | null {
  switch (status) {
    case "succeeded":
      return { label: "No merge conflicts", tone: "green" };
    case "conflicts":
      return { label: "Merge conflicts", tone: "red" };
    case "queued":
      return { label: "Checking for conflicts", tone: "neutral" };
    case "rejectedByPolicy":
      return { label: "Rejected by policy", tone: "red" };
    case "failure":
      return { label: "Merge check failed", tone: "red" };
    default:
      return null;
  }
}

export function changeTypeBadge(type: FileChangeType): { letter: string; label: string; tone: Tone } {
  switch (type) {
    case "add":
      return { letter: "A", label: "Added", tone: "green" };
    case "delete":
      return { letter: "D", label: "Deleted", tone: "red" };
    case "rename":
      return { letter: "R", label: "Renamed", tone: "violet" };
    default:
      return { letter: "M", label: "Modified", tone: "blue" };
  }
}

/** Thread count per file path, for the file list. */
export function threadCountsByPath(threads: PullRequestThread[]): Map<string, number> {
  const counts = new Map<string, number>();
  for (const thread of threads) {
    if (thread.kind === "file" && thread.filePath) {
      counts.set(thread.filePath, (counts.get(thread.filePath) ?? 0) + 1);
    }
  }
  return counts;
}

export function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1) || path;
}

export function directory(path: string): string {
  const index = path.lastIndexOf("/");
  return index > 0 ? path.slice(0, index) : "";
}
