export interface DisplayField {
  label: string
  value: string
}

export interface Notice {
  title: string
  message: string
}

export interface PipelineTerminology {
  item: string
  items: string
  group: string
  groups: string
  group_title: string
  source_title: string
  sources_title: string
  history: string
  trigger: string
  run: string
  runs: string
  organization_label: string
  section_count_label: string
}

export interface WizardCopy {
  credentials_description: string
  name_placeholder: string
  permissions_title: string
}

export interface ActionCopy {
  label: string
  compact_label: string
  title: string
  success: Notice
  error_title: string
  note: string | null
}

export interface ViewCopy {
  title: string
  list_title: string
  search_placeholder: string
  matching_empty: Notice
  hidden_empty: Notice
  empty: Notice
  pinned_empty: Notice
}

export interface RunDisplay {
  title: string
  message: string | null
  context: DisplayField[]
  started_label: string
}

export interface SectionPresentation {
  organization_label: string
  count_label: string
}
