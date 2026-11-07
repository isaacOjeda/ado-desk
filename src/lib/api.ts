import { invoke } from "@tauri-apps/api/core";

export interface Session {
  organization: string;
  userId: string;
  displayName: string;
}

export interface WorkItem {
  id: number;
  title: string;
  workItemType: string;
  state: string;
  project: string;
  priority: number | null;
  changedAt: string | null;
  url: string;
}

export type PullRequestRole = "author" | "reviewer";

export interface PullRequest {
  id: number;
  title: string;
  project: string;
  repository: string;
  sourceBranch: string;
  targetBranch: string;
  author: string;
  createdAt: string;
  isDraft: boolean;
  role: PullRequestRole;
  myVote: number | null;
  approvals: number;
  reviewerCount: number;
  url: string;
}

export type ApiErrorKind =
  | "invalidOrganization"
  | "unauthorized"
  | "forbidden"
  | "notFound"
  | "notSignedIn"
  | "api"
  | "network"
  | "decode"
  | "credentials";

export class ApiError extends Error {
  constructor(
    public readonly kind: ApiErrorKind,
    message: string,
  ) {
    super(message);
  }
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (raw) {
    if (raw && typeof raw === "object" && "kind" in raw && "message" in raw) {
      const { kind, message } = raw as { kind: ApiErrorKind; message: string };
      throw new ApiError(kind, message);
    }
    throw new ApiError("api", String(raw));
  }
}

export const api = {
  login: (organization: string, pat: string) => call<Session>("login", { organization, pat }),
  restoreSession: () => call<Session | null>("restore_session"),
  logout: () => call<void>("logout"),
  listWorkItems: () => call<WorkItem[]>("list_work_items"),
  listPullRequests: () => call<PullRequest[]>("list_pull_requests"),
};
