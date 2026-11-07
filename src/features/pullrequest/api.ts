// Typed access to the pull request commands in `src-tauri/src/pull_requests.rs`.
// Every `*Html` field is already sanitized in Rust; file contents are plain text.
import { call, type PullRequest, type WorkItem } from "../../lib/api";

export type PullRequestStatus = "active" | "draft" | "completed" | "abandoned";

export interface Reviewer {
  id: string;
  name: string;
  vote: number;
  isRequired: boolean;
  hasDeclined: boolean;
  isGroup: boolean;
}

export interface PullRequestDetail {
  id: number;
  title: string;
  descriptionHtml: string;
  status: PullRequestStatus;
  isDraft: boolean;
  author: string;
  createdAt: string;
  closedAt: string | null;
  sourceBranch: string;
  targetBranch: string;
  mergeStatus: string | null;
  repository: string;
  project: string;
  reviewers: Reviewer[];
  labels: string[];
  workItems: WorkItem[];
  url: string;
}

export type ThreadKind = "general" | "file" | "system";

export interface ThreadComment {
  id: number;
  parentId: number | null;
  author: string;
  publishedAt: string | null;
  updatedAt: string | null;
  /** Sanitized HTML; empty for system comments. */
  html: string;
  /** Plain text for system comments. Render as a text node. */
  text: string | null;
  isSystem: boolean;
}

export interface PullRequestThread {
  id: number;
  kind: ThreadKind;
  status: string | null;
  publishedAt: string | null;
  filePath: string | null;
  rightLine: number | null;
  leftLine: number | null;
  comments: ThreadComment[];
}

export type FileChangeType = "add" | "edit" | "delete" | "rename";

export interface FileChange {
  path: string;
  originalPath: string | null;
  changeType: FileChangeType;
  objectId: string | null;
  originalObjectId: string | null;
}

export interface PullRequestFiles {
  iterationId: number | null;
  files: FileChange[];
  truncated: boolean;
}

export type DiffLineKind = "context" | "add" | "delete";

export interface DiffLine {
  kind: DiffLineKind;
  oldLine: number | null;
  newLine: number | null;
  text: string;
}

export interface DiffHunk {
  oldStart: number;
  oldLines: number;
  newStart: number;
  newLines: number;
  lines: DiffLine[];
}

export interface FileDiff {
  hunks: DiffHunk[];
  additions: number;
  deletions: number;
  binary: boolean;
  tooLarge: boolean;
}

type PullRequestRef = Pick<PullRequest, "project" | "repositoryId" | "id">;

const prArgs = ({ project, repositoryId, id }: PullRequestRef) => ({ project, repositoryId, id });

export const pullRequestApi = {
  detail: (pr: PullRequestRef) => call<PullRequestDetail>("get_pull_request", prArgs(pr)),
  threads: (pr: PullRequestRef) => call<PullRequestThread[]>("list_pull_request_threads", prArgs(pr)),
  files: (pr: PullRequestRef) => call<PullRequestFiles>("list_pull_request_files", prArgs(pr)),
  fileDiff: (pr: PullRequestRef, file: FileChange) =>
    call<FileDiff>("get_pull_request_file_diff", {
      project: pr.project,
      repositoryId: pr.repositoryId,
      objectId: file.objectId,
      originalObjectId: file.originalObjectId,
    }),
};
