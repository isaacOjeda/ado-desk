import type { PullRequestFilter, PullRequestRoleFilter, PullRequestStatusFilter } from "./api";
import { FilterBar, FilterSelect, type Option } from "./FilterControls";
import { countPullRequestFilters, DEFAULT_PULL_REQUEST_FILTER, normalizePullRequestFilter } from "./model";
import { projectOptions } from "./WorkItemFilterBar";

interface Props {
  filter: PullRequestFilter;
  onChange: (filter: PullRequestFilter) => void;
  /** Projects of the active organization; `null` while loading. */
  projects: string[] | null;
}

const STATUS_OPTIONS: Option[] = [
  { value: "active", label: "Active" },
  { value: "completed", label: "Completed" },
  { value: "abandoned", label: "Abandoned" },
  { value: "all", label: "All" },
];

function roleOptions(hasProject: boolean): Option[] {
  const roles: Option[] = [
    { value: "mine", label: "Mine" },
    { value: "created", label: "Created by me" },
    { value: "reviewing", label: "Reviewing" },
  ];
  return hasProject ? [...roles, { value: "everyone", label: "Everyone" }] : roles;
}

export function PullRequestFilterBar({ filter, onChange, projects }: Props) {
  const update = (patch: Partial<PullRequestFilter>) => onChange(normalizePullRequestFilter({ ...filter, ...patch }));

  return (
    <FilterBar activeCount={countPullRequestFilters(filter)} onReset={() => onChange(DEFAULT_PULL_REQUEST_FILTER)}>
      <FilterSelect
        label="Project"
        value={filter.project ?? ""}
        options={projectOptions(projects, filter.project)}
        isDefault={filter.project === null}
        onChange={(value) => update({ project: value === "" ? null : value })}
      />
      <FilterSelect
        label="Status"
        value={filter.status}
        options={STATUS_OPTIONS}
        isDefault={filter.status === "active"}
        onChange={(value) => update({ status: value as PullRequestStatusFilter })}
      />
      <FilterSelect
        label="Role"
        value={filter.role}
        options={roleOptions(filter.project !== null)}
        isDefault={filter.role === "mine"}
        onChange={(value) => update({ role: value as PullRequestRoleFilter })}
        title={filter.project ? undefined : "Select a project to see everyone's pull requests"}
      />
    </FilterBar>
  );
}
