import type { ActionAvailability, PipelineActionState, RunActionState, RunPace } from '../types/actions'
import type { PluginPresentation, ViewPresentation } from '../types/plugin_presentation'
import type { ActionCopy, PipelineTerminology, RunDisplay, SectionPresentation, ViewCopy } from '../types/presentation'

const allowed: ActionAvailability = { allowed: true, reason: null }

export function pipelineActionsFixture(overrides: Partial<PipelineActionState> = {}): PipelineActionState {
  return {
    trigger: allowed,
    cancel: allowed,
    force_cancel: { allowed: false, reason: null },
    enable: allowed,
    rerun_strategy: 'replay_inputs',
    trigger_icon: 'play',
    target: null,
    ...overrides,
  }
}

export function runActionsFixture(overrides: Partial<RunActionState> = {}): RunActionState {
  return {
    cancel: allowed,
    force_cancel: { allowed: false, reason: null },
    rerun: allowed,
    rerun_failed: allowed,
    review: { allowed: false, reason: null },
    watch: true,
    ...overrides,
  }
}

export function runDisplayFixture(overrides: Partial<RunDisplay> = {}): RunDisplay {
  return { title: '#1', message: null, context: [], started_label: 'Started', ...overrides }
}

export function runPaceFixture(overrides: Partial<RunPace> = {}): RunPace {
  return { live: false, typical_duration_seconds: null, past_usual: false, ...overrides }
}

export function runPresentationFixture(): { actions: RunActionState; display: RunDisplay; pace: RunPace } {
  return { actions: runActionsFixture(), display: runDisplayFixture(), pace: runPaceFixture() }
}

const terminology: PipelineTerminology = {
  item: 'pipeline',
  items: 'pipelines',
  group: 'repository',
  groups: 'repositories',
  group_title: 'Repository',
  source_title: 'Repository',
  sources_title: 'Repositories',
  history: 'Run history',
  trigger: 'Run workflow',
  run: 'run',
  runs: 'runs',
  organization_label: 'Organization',
  section_count_label: 'Workflows',
}

const notice = { title: 'Nothing here', message: 'Nothing to show.' }

const viewCopy: ViewCopy = {
  title: 'Pipelines',
  list_title: 'All pipelines',
  search_placeholder: 'Search pipelines…',
  matching_empty: notice,
  hidden_empty: notice,
  empty: notice,
  pinned_empty: notice,
}

const actionCopy: ActionCopy = {
  label: 'Run workflow',
  compact_label: 'Run',
  title: 'Run workflow',
  success: { title: 'Workflow triggered', message: 'Workflow has been triggered successfully' },
  error_title: "Couldn't trigger workflow",
  note: null,
}

export function viewPresentationFixture(): ViewPresentation {
  return { terminology, status_options: [], copy: viewCopy }
}

export function sectionPresentationFixture(): SectionPresentation {
  return { organization_label: 'Organization', count_label: 'Workflows' }
}

export function pluginPresentationFixture(): PluginPresentation {
  return {
    terminology,
    status_options: [],
    wizard: {
      credentials_description: 'Connect your account',
      name_placeholder: 'e.g. Provider (work)',
      permissions_title: 'Token permissions',
    },
    view: viewCopy,
    trigger: actionCopy,
    rerun: { ...actionCopy, label: 'Rerun workflow', title: 'Rerun workflow' },
    history_note: null,
    empty_history_description: 'No runs yet',
    run_details_title: 'Run details',
    progress_title: 'Run in progress',
    archived_notice: null,
    force_cancel_confirmation: "You can't undo this.",
  }
}
