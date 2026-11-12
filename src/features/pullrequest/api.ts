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
  /** The signed-in user's vote (0 when they have not voted). */
  myVote: number;
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

/** `right`: the changed version (added and unchanged lines); `left`: the original (deleted lines). */
export type DiffSide = "left" | "right";

/** Mirrors `pull_request_actions::LineSelection`. */
export interface LineSelection {
  filePath: string;
  side: DiffSide;
  startLine: number;
  endLine: number;
  /** Column just past the last selected character of `endLine` (1-based). */
  endOffset: number;
}

/** Rendered Markdown of one file version; `html` is sanitized in Rust. */
export interface FilePreview {
  html: string;
  binary: boolean;
  tooLarge: boolean;
}

/** Markdown files get a rendered preview next to their diff. */
export function isMarkdownFile(path: string): boolean {
  return /\.(md|markdown)$/i.test(path);
}

/** The blob to preview: the changed version, or the original one for a deleted file. */
export function previewObjectId(file: FileChange): string | null {
  return file.changeType === "delete" ? file.originalObjectId : file.objectId;
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
  /** Sets the signed-in user's vote: 10, 5, 0 (reset), -5 or -10. */
  vote: (pr: PullRequestRef, vote: number) => call<void>("vote_pull_request", { ...prArgs(pr), vote }),
  /** Starts a thread on the selected lines; `content` is Markdown, 1-4000 characters. */
  lineComment: (pr: PullRequestRef, selection: LineSelection, content: string) =>
    call<void>("add_pull_request_line_comment", { ...prArgs(pr), selection, content }),
  filePreview: (pr: PullRequestRef, objectId: string) =>
    call<FilePreview>("get_pull_request_file_preview", { project: pr.project, repositoryId: pr.repositoryId, objectId }),
};
