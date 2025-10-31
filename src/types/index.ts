export type PipelineStatus =
  | 'success'
  | 'failed'
  | 'running'
  | 'pending'
  | 'cancelled'
  | 'skipped';

export interface AvailablePipeline {
  id: string;
  name: string;
  description: string | null;
  organization: string | null;
  repository: string | null;
}

export interface Pipeline {
  id: string;
  provider_id: number;
  provider_type: string;
  name: string;
  status: PipelineStatus;
  last_run: string | null;
  last_updated: string;
  repository: string;
  branch: string | null;
  workflow_file: string | null;
}

export interface PipelineRun {
  id: string;
  pipeline_id: string;
  run_number: number;
  status: PipelineStatus;
  started_at: string;
  concluded_at: string | null;
  duration_seconds: number | null;
  logs_url: string;
  commit_sha: string;
  commit_message: string | null;
  branch: string;
  actor: string;
  inputs?: Record<string, any>;
}

export interface PaginatedRunHistory {
  runs: PipelineRun[];
  total_count: number;
  has_more: boolean;
  is_complete: boolean;
  page: number;
  page_size: number;
  total_pages: number;
}

export interface ProviderConfig {
  id?: number;
  name: string;
  provider_type: string;
  token: string;
  config: Record<string, string>;
  refresh_interval: number;
}

export interface ProviderSummary {
  id: number;
  name: string;
  provider_type: string;
  icon: string | null;
  pipeline_count: number;
  last_updated: string | null;
  refresh_interval: number;
}

export interface TriggerParams {
  workflow_id: string;
  inputs?: Record<string, any>;
}

interface PluginCapabilities {
  pipelines: boolean;
  pipeline_runs: boolean;
  trigger: boolean;
  agents: boolean;
  artifacts: boolean;
  queues: boolean;
  custom_tables: boolean;
}

type ConfigFieldType = 'Text' | 'TextArea' | 'Password' | 'Number' | 'Select' | 'Checkbox';

export interface ConfigField {
  key: string;
  label: string;
  description: string | null;
  field_type: ConfigFieldType;
  required: boolean;
  default_value: string | null;
  options: string[] | null;
  validation_regex: string | null;
  validation_message: string | null;
}

interface ConfigSchema {
  fields: ConfigField[];
}

export interface PluginMetadata {
  name: string;
  provider_type: string;
  version: string;
  description: string;
  author: string | null;
  icon: string | null;
  config_schema: ConfigSchema;
  capabilities: PluginCapabilities;
}

export interface WorkflowParameter {
  name: string;
  label: string | null;
  description: string | null;
  type: 'string' | 'boolean' | 'choice' | 'number';
  default?: string | number | boolean | null;
  options?: string[];
  required: boolean;
}
