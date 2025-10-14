export interface BundleInfo {
  id: string;
  path: string;
  name: string;
  pipeline_name: string;
  is_dev_path: boolean;
  commands: Record<string, CommandInfo>;
  entry: EntryInfo;
  output: RefInfo;
}

export interface PipelineMetadata {
  name: string;
  is_default: boolean;
  is_dev: boolean;
}

export interface EntryInfo {
  value_type: string;
}

export interface RefInfo {
  ref: string;
}

export interface CommandInfo {
  module: string;
  command: string;
  returns: string;
}

export interface PipelineStep {
  window_id: string;
  tab_id: string;
  execution_id: string;
  step_index: number;
  command_key: string;
  command: {
    module: string;
    command: string;
    id?: string;
    params?: Record<string, unknown>;
  };
  command_display: string;
  event_html: string;
  kind?: string;
  value_type?: string;
  event_rich_html?: string;
}

export interface TabInfo {
  tab_id: string;
  bundle_name: string | null;
  current_view: string;
}

export interface WindowStateInfo {
  window_id: string;
  tabs: TabInfo[];
  active_tab_index: number;
}

export interface TabData {
  tab_id: string;
  bundle_info: BundleInfo | null;
  current_view: string;
  pipeline_input: string;
  fluent_file: string | null;
  fluent_message: string | null;
  fluent_args: Record<string, string>;
}
