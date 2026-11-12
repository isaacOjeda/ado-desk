import { FilterBar, FilterSelect, type Option } from "../filters/FilterControls";
import type { InboxItem, InboxKind } from "./api";
import { countInboxFilters, DEFAULT_INBOX_FILTER, inboxProjects, sameProject, type InboxFilter, type InboxProject } from "./filter";

const KIND_OPTIONS: Option[] = [
  { value: "all", label: "All" },
  { value: "reviewRequest", label: "Review requests" },
  { value: "assignment", label: "Assignments" },
  { value: "comment", label: "Comments" },
];

const STATE_OPTIONS: Option[] = [
  { value: "all", label: "All" },
  { value: "unread", label: "Unread" },
];

const ALL_PROJECTS = "";

interface Props {
  filter: InboxFilter;
  onChange: (filter: InboxFilter) => void;
  /** Inbox items; the project options come from them. `null` while loading. */
  items: InboxItem[] | null;
}

export function InboxFilterBar({ filter, onChange, items }: Props) {
  const projects = inboxProjects(items ?? []);
  // Keep the selected project listed even when no current item belongs to it.
  const selected = filter.project;
  if (selected && !projects.some((p) => sameProject(p, selected))) projects.push(selected);
  const multipleOrganizations = new Set(projects.map((p) => p.organization.toLowerCase())).size > 1;

  const projectOptions: Option[] = [
    { value: ALL_PROJECTS, label: "All projects" },
    ...projects.map((p, index) => ({
      value: String(index),
      label: multipleOrganizations ? `${p.organization} / ${p.project}` : p.project,
    })),
  ];
  const selectedIndex = selected ? projects.findIndex((p) => sameProject(p, selected)) : -1;

  return (
    <FilterBar activeCount={countInboxFilters(filter)} onReset={() => onChange(DEFAULT_INBOX_FILTER)}>
      <FilterSelect
        label="Project"
        value={selectedIndex < 0 ? ALL_PROJECTS : String(selectedIndex)}
        options={projectOptions}
        isDefault={filter.project === null}
        onChange={(value) => {
          const project: InboxProject | undefined = value === ALL_PROJECTS ? undefined : projects[Number(value)];
          onChange({ ...filter, project: project ?? null });
        }}
      />
      <FilterSelect
        label="Type"
        value={filter.kind}
        options={KIND_OPTIONS}
        isDefault={filter.kind === "all"}
        onChange={(value) => onChange({ ...filter, kind: value as InboxKind | "all" })}
      />
      <FilterSelect
        label="Status"
        value={filter.state}
        options={STATE_OPTIONS}
        isDefault={filter.state === "all"}
        onChange={(value) => onChange({ ...filter, state: value === "unread" ? "unread" : "all" })}
      />
    </FilterBar>
  );
}
