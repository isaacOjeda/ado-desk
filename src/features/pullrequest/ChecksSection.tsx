// "Checks" section of the pull request side panel: policies and builds with their state.
import { openUrl } from "@tauri-apps/plugin-opener";
import { Badge, Spinner } from "../../components/ui";
import type { Resource } from "../../lib/useResource";
import { checkStateBadge, type BuildCheck, type PolicyCheck, type PullRequestChecks } from "./checks";

function CheckRow({
  name,
  detail,
  state,
  url,
}: {
  name: string;
  detail: string;
  state: PolicyCheck["state"];
  url: string | null;
}) {
  const badge = checkStateBadge(state);
  const body = (
    <>
      <span className="pr-check-main">
        <span className="pr-check-name" title={name}>
          {name}
        </span>
        <span className="pr-check-detail">{detail}</span>
      </span>
      <Badge tone={badge.tone}>{badge.label}</Badge>
    </>
  );
  return (
    <li>
      {url ? (
        <button className="pr-check is-link" onClick={() => void openUrl(url)} title="Open the run in the browser">
          {body}
        </button>
      ) : (
        <div className="pr-check">{body}</div>
      )}
    </li>
  );
}

function policyDetail(policy: PolicyCheck) {
  const requirement = policy.blocking ? "Required" : "Optional";
  return policy.kind === "build" ? `${requirement} · Build validation` : policy.kind === "status" ? `${requirement} · Status` : requirement;
}

function buildDetail(build: BuildCheck) {
  const source = build.source === "pullRequest" ? "Pull request" : "Branch CI";
  return build.number ? `${source} · ${build.number}` : source;
}

export function ChecksSection({ checks }: { checks: Resource<PullRequestChecks> }) {
  return (
    <section>
      <h3 className="section-title">Checks</h3>
      {checks.error ? (
        <p className="rich-empty">Checks unavailable.</p>
      ) : checks.data === null ? (
        <Spinner />
      ) : (
        <>
          {checks.data.policies === null ? (
            <p className="rich-empty">Policy status unavailable.</p>
          ) : checks.data.policies.length === 0 ? (
            <p className="rich-empty">No policies</p>
          ) : (
            <ul className="pr-plain-list" aria-label="Policies">
              {checks.data.policies.map((policy, index) => (
                <CheckRow
                  key={`${policy.name}-${index}`}
                  name={policy.name}
                  detail={policyDetail(policy)}
                  state={policy.state}
                  url={policy.url}
                />
              ))}
            </ul>
          )}
          {checks.data.builds === null ? (
            <p className="rich-empty">Build status unavailable. The token may lack Build (Read).</p>
          ) : (
            checks.data.builds.length > 0 && (
              <ul className="pr-plain-list" aria-label="Builds">
                {checks.data.builds.map((build) => (
                  <CheckRow
                    key={build.id}
                    name={build.definition}
                    detail={buildDetail(build)}
                    state={build.state}
                    url={build.url}
                  />
                ))}
              </ul>
            )
          )}
        </>
      )}
    </section>
  );
}
