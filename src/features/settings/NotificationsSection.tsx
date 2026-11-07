// Notifications settings: background checks for review requests, assignments and comments.
import { useState } from "react";
import type { NotificationSettings, Settings } from "../../lib/settings";
import { inboxApi } from "../inbox/api";
import "./appearance.css";

const INTERVALS = [1, 2, 5, 10, 15, 30, 60];

const SIGNALS: { key: "reviewRequests" | "assignments" | "comments"; label: string; hint: string }[] = [
  { key: "reviewRequests", label: "Review requests", hint: "Pull requests waiting for your vote." },
  { key: "assignments", label: "Assignments", hint: "Open work items newly assigned to you." },
  {
    key: "comments",
    label: "Comments",
    hint: "New comments by others on your active pull requests and your assigned work items.",
  },
];

interface Props {
  settings: Settings;
  onChange: (next: Settings) => void;
}

function Switch({ on, labelledBy, disabled, onToggle }: { on: boolean; labelledBy: string; disabled?: boolean; onToggle: () => void }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-labelledby={labelledBy}
      disabled={disabled}
      className={`appearance-switch${on ? " is-on" : ""}`}
      onClick={onToggle}
    >
      <span className="appearance-switch-thumb" />
    </button>
  );
}

export function NotificationsSection({ settings, onChange }: Props) {
  const notifications = settings.notifications;
  const [checking, setChecking] = useState(false);
  const update = (patch: Partial<NotificationSettings>) =>
    onChange({ ...settings, notifications: { ...notifications, ...patch } });
  const off = !notifications.enabled;

  const checkNow = () => {
    setChecking(true);
    inboxApi
      .checkNow()
      .catch(() => {})
      .finally(() => window.setTimeout(() => setChecking(false), 1500));
  };

  return (
    <div className="appearance">
      <section className="appearance-row">
        <div className="appearance-text">
          <h3 className="appearance-label" id="notifications-enabled">
            Notifications
          </h3>
          <p className="appearance-hint">
            Checks every organization in the background, also when the window is closed, and adds what is new to the
            Inbox. The first check only records what is already there.
          </p>
        </div>
        <Switch
          on={notifications.enabled}
          labelledBy="notifications-enabled"
          onToggle={() => update({ enabled: !notifications.enabled })}
        />
      </section>

      <section className={`appearance-row${off ? " is-disabled" : ""}`}>
        <div className="appearance-text">
          <h3 className="appearance-label" id="notifications-interval">
            Check every
          </h3>
          <p className="appearance-hint">Each check makes a few requests per organization.</p>
        </div>
        <div className="notifications-interval">
          <select
            aria-labelledby="notifications-interval"
            value={notifications.intervalMinutes}
            disabled={off}
            onChange={(event) => update({ intervalMinutes: Number(event.target.value) })}
          >
            {INTERVALS.map((minutes) => (
              <option key={minutes} value={minutes}>
                {minutes === 1 ? "1 minute" : `${minutes} minutes`}
              </option>
            ))}
          </select>
          <button type="button" className="btn btn-sm" disabled={off || checking} onClick={checkNow}>
            {checking ? "Checking…" : "Check now"}
          </button>
        </div>
      </section>

      {SIGNALS.map((signal) => (
        <section key={signal.key} className={`appearance-row${off ? " is-disabled" : ""}`}>
          <div className="appearance-text">
            <h3 className="appearance-label" id={`notifications-${signal.key}`}>
              {signal.label}
            </h3>
            <p className="appearance-hint">{signal.hint}</p>
          </div>
          <Switch
            on={notifications[signal.key]}
            labelledBy={`notifications-${signal.key}`}
            disabled={off}
            onToggle={() => update({ [signal.key]: !notifications[signal.key] })}
          />
        </section>
      ))}
    </div>
  );
}
