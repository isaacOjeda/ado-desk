// Container: loads the sprint, people and type options of the selected project.
import { useCallback } from "react";
import { useResource } from "../../lib/useResource";
import {
  filtersApi,
  type Assignee,
  type Iteration,
  type IterationFilter,
  type TeamMember,
  type WorkItemFilter,
  type WorkItemStateFilter,
} from "./api";
import { FilterBar, FilterSelect, MultiSelect, type Option } from "./FilterControls";
import { countWorkItemFilters, DEFAULT_WORK_ITEM_FILTER, normalizeWorkItemFilter } from "./model";

interface Props {
  filter: WorkItemFilter;
  onChange: (filter: WorkItemFilter) => void;
  /** Projects of the active organization; `null` while loading. */
  projects: string[] | null;
}

const ALL = "";
const STATE_OPTIONS: Option[] = [
  { value: "open", label: "Open" },
  { value: "closed", label: "Closed" },
  { value: "all", label: "All" },
];

export function projectOptions(projects: string[] | null, selected: string | null): Option[] {
  const names = projects ?? [];
  const extra = selected && !names.includes(selected) ? [selected] : [];
  return [{ value: ALL, label: "All projects" }, ...[...names, ...extra].map((p) => ({ value: p, label: p }))];
}

function assigneeValue(assignee: Assignee): string {
  return assignee.kind === "person" ? `person:${assignee.uniqueName}` : assignee.kind;
}

function iterationValue(iteration: IterationFilter): string {
  return iteration.kind === "path" ? `path:${iteration.path}` : iteration.kind;
}

export function WorkItemFilterBar({ filter, onChange, projects }: Props) {
  const project = filter.project;
  const iterations = useResource<Iteration[]>(
    useCallback(() => (project ? filtersApi.listIterations(project) : Promise.resolve([])), [project]),
  );
  const members = useResource<TeamMember[]>(
    useCallback(() => (project ? filtersApi.listTeamMembers(project) : Promise.resolve([])), [project]),
  );
  const types = useResource(useCallback(() => filtersApi.listWorkItemTypes(project), [project]));

  const update = (patch: Partial<WorkItemFilter>) => onChange(normalizeWorkItemFilter({ ...filter, ...patch }, filter));

  const people = (members.data ?? []).map((m) => ({ value: `person:${m.uniqueName}`, label: m.displayName }));
  const selectedPerson = filter.assignee.kind === "person" ? filter.assignee : null;
  if (selectedPerson && !people.some((p) => p.value === assigneeValue(selectedPerson))) {
    people.unshift({
      value: assigneeValue(selectedPerson),
      label: selectedPerson.displayName ?? selectedPerson.uniqueName,
    });
  }
  const assigneeOptions: Option[] = [
    { value: "me", label: "Me" },
    { value: "anyone", label: "Anyone" },
    { value: "unassigned", label: "Unassigned" },
    ...people,
  ];

  // Ignore iterations still shown from a previously selected project.
  const sprints = (iterations.data ?? []).filter(
    (it) => project && (it.path === project || it.path.toLowerCase().startsWith(`${project.toLowerCase()}\\`)),
  );
  const iterationOptions: Option[] = [
    { value: "any", label: "Any" },
    { value: "current", label: "Current" },
    ...sprints.map((it) => ({ value: `path:${it.path}`, label: it.isCurrent ? `${it.name} (current)` : it.name })),
  ];
  if (filter.iteration.kind === "path" && !iterationOptions.some((o) => o.value === iterationValue(filter.iteration))) {
    const path = filter.iteration.path;
    iterationOptions.push({ value: `path:${path}`, label: path.split("\\").pop() ?? path });
  }

  const onAssignee = (value: string) => {
    if (value === "me" || value === "anyone" || value === "unassigned") return update({ assignee: { kind: value } });
    const uniqueName = value.slice("person:".length);
    const member = members.data?.find((m) => m.uniqueName === uniqueName);
    update({ assignee: { kind: "person", uniqueName, displayName: member?.displayName ?? selectedPerson?.displayName } });
  };

  const onIteration = (value: string) =>
    update({
      iteration:
        value === "any" || value === "current" ? { kind: value } : { kind: "path", path: value.slice("path:".length) },
    });

  return (
    <FilterBar activeCount={countWorkItemFilters(filter)} onReset={() => onChange(DEFAULT_WORK_ITEM_FILTER)}>
      <FilterSelect
        label="Project"
        value={project ?? ALL}
        options={projectOptions(projects, project)}
        isDefault={project === null}
        onChange={(value) => update({ project: value === ALL ? null : value })}
      />
      <FilterSelect
        label="Assigned to"
        value={assigneeValue(filter.assignee)}
        options={assigneeOptions}
        isDefault={filter.assignee.kind === "me"}
        onChange={onAssignee}
        title={project ? undefined : "Select a project to pick a team member"}
      />
      <FilterSelect
        label="Sprint"
        value={iterationValue(filter.iteration)}
        options={iterationOptions}
        isDefault={filter.iteration.kind === "any"}
        disabled={!project}
        onChange={onIteration}
        title={project ? undefined : "Select a project to filter by sprint"}
      />
      <MultiSelect
        label="Type"
        options={types.data ?? []}
        selected={filter.types}
        loading={types.loading}
        onChange={(selected) => update({ types: selected })}
      />
      <FilterSelect
        label="State"
        value={filter.state}
        options={STATE_OPTIONS}
        isDefault={filter.state === "open"}
        onChange={(value) => update({ state: value as WorkItemStateFilter })}
      />
    </FilterBar>
  );
}
