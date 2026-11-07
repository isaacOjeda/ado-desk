const relative = new Intl.RelativeTimeFormat("en", { numeric: "auto" });

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ["year", 365 * 24 * 3600],
  ["month", 30 * 24 * 3600],
  ["week", 7 * 24 * 3600],
  ["day", 24 * 3600],
  ["hour", 3600],
  ["minute", 60],
];

export function timeAgo(iso: string | null, now = Date.now()): string {
  if (!iso) return "";
  const seconds = (new Date(iso).getTime() - now) / 1000;
  for (const [unit, size] of UNITS) {
    if (Math.abs(seconds) >= size) return relative.format(Math.round(seconds / size), unit);
  }
  return "just now";
}

export function initials(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]!.toUpperCase())
    .join("");
}

export type Tone = "neutral" | "blue" | "green" | "amber" | "red" | "violet" | "orange";

export function workItemTypeTone(type: string): Tone {
  switch (type.toLowerCase()) {
    case "bug":
      return "red";
    case "task":
      return "amber";
    case "feature":
      return "violet";
    case "epic":
      return "orange";
    case "user story":
    case "product backlog item":
      return "blue";
    default:
      return "neutral";
  }
}

export function stateTone(state: string): Tone {
  const s = state.toLowerCase();
  if (["new", "to do", "proposed"].includes(s)) return "neutral";
  if (["resolved", "ready", "in review", "testing"].includes(s)) return "violet";
  if (["blocked", "on hold"].includes(s)) return "red";
  return "blue";
}

/** Badge for closed pull requests; active ones get none. */
export function pullRequestStatusBadge(status: string | undefined): { label: string; tone: Tone } | null {
  switch (status) {
    case "completed":
      return { label: "Completed", tone: "green" };
    case "abandoned":
      return { label: "Abandoned", tone: "neutral" };
    default:
      return null;
  }
}

export function voteLabel(vote: number | null): { label: string; tone: Tone } {
  switch (vote) {
    case 10:
      return { label: "Approved", tone: "green" };
    case 5:
      return { label: "Approved w/ suggestions", tone: "green" };
    case -5:
      return { label: "Waiting for author", tone: "amber" };
    case -10:
      return { label: "Rejected", tone: "red" };
    case 0:
      return { label: "Needs your review", tone: "blue" };
    default:
      return { label: "", tone: "neutral" };
  }
}
