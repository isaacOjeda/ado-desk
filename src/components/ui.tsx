import type { MouseEvent, ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { initials, timeAgo, type Tone } from "../lib/format";

/**
 * Renders HTML that was ALREADY sanitized in Rust (`rich_text.rs`).
 * Never pass unsanitized content here. Links open in the system browser.
 */
export function RichText({ html, className = "" }: { html: string; className?: string }) {
  const onClick = (event: MouseEvent<HTMLDivElement>) => {
    const anchor = (event.target as HTMLElement).closest("a");
    if (!anchor) return;
    event.preventDefault();
    const href = anchor.getAttribute("href");
    if (href && /^https:\/\//i.test(href)) void openUrl(href);
  };
  return <div className={`rich-text ${className}`} onClick={onClick} dangerouslySetInnerHTML={{ __html: html }} />;
}

export function Avatar({ name, size = 28 }: { name: string; size?: number }) {
  return (
    <span className="avatar" style={{ width: size, height: size, fontSize: Math.round(size * 0.4) }} title={name}>
      {initials(name)}
    </span>
  );
}

/** One comment in a discussion, shared by work items and pull requests. */
export function CommentCard({
  author,
  date,
  html,
  footer,
}: {
  author: string;
  date: string | null;
  html: string;
  footer?: ReactNode;
}) {
  return (
    <article className="comment">
      <Avatar name={author} />
      <div className="comment-body">
        <header className="comment-header">
          <span className="comment-author">{author}</span>
          {date && (
            <time className="comment-date" dateTime={date} title={new Date(date).toLocaleString()}>
              {timeAgo(date)}
            </time>
          )}
        </header>
        <RichText html={html} />
        {footer}
      </div>
    </article>
  );
}

/** Header shared by detail views: back button, title row and actions. */
export function DetailHeader({
  onBack,
  eyebrow,
  title,
  actions,
  children,
}: {
  onBack: () => void;
  eyebrow: ReactNode;
  title: string;
  actions?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <header className="detail-header">
      <div className="detail-topbar" data-tauri-drag-region>
        <button className="btn btn-ghost btn-sm" onClick={onBack} title="Back (Esc)">
          ← Back
        </button>
        <span className="detail-eyebrow">{eyebrow}</span>
        <span className="detail-actions">{actions}</span>
      </div>
      <h1 className="detail-title">{title}</h1>
      {children}
    </header>
  );
}

export function OpenInBrowser({ url }: { url: string }) {
  return (
    <button className="btn btn-ghost btn-sm" onClick={() => void openUrl(url)}>
      Open in browser ↗
    </button>
  );
}

export function Badge({ tone = "neutral", children }: { tone?: Tone; children: ReactNode }) {
  return <span className={`badge tone-${tone}`}>{children}</span>;
}

export function Dot({ tone }: { tone: Tone }) {
  return <span className={`dot tone-${tone}`} aria-hidden />;
}

export function Spinner() {
  return <span className="spinner" role="status" aria-label="Loading" />;
}

export function EmptyState({ title, hint }: { title: string; hint?: string }) {
  return (
    <div className="empty">
      <p className="empty-title">{title}</p>
      {hint && <p className="empty-hint">{hint}</p>}
    </div>
  );
}

export function ErrorBanner({ message, onRetry }: { message: string; onRetry?: () => void }) {
  return (
    <div className="error-banner" role="alert">
      <span>{message}</span>
      {onRetry && (
        <button className="btn btn-ghost" onClick={onRetry}>
          Retry
        </button>
      )}
    </div>
  );
}
