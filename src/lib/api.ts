import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface Session {
  organization: string;
  userId: string;
  displayName: string;
  /** Sign-in email, when Azure DevOps exposes one. */
  email: string | null;
}

/** Emitted by Rust to every window after sign-in or sign-out; payload is the session or null. */
export const SESSION_CHANGED_EVENT = "session-changed";

export function onSessionChanged(handler: (session: Session | null) => void): Promise<UnlistenFn> {
  return listen<Session | null>(SESSION_CHANGED_EVENT, (event) => handler(event.payload));
}

/**
 * Subscribes from a React effect. Handles `listen` resolving after cleanup
 * (StrictMode double effects) so no listener leaks. Returns the cleanup.
 */
export function subscribe(start: () => Promise<UnlistenFn>): () => void {
  let unlisten: UnlistenFn | undefined;
  let cancelled = false;
  start()
    .then((stop) => (cancelled ? stop() : (unlisten = stop)))
    .catch(() => {});
  return () => {
    cancelled = true;
    unlisten?.();
  };
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
  repositoryId: string;
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

/** Invokes a Tauri command and normalizes its error into `ApiError`. */
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
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
