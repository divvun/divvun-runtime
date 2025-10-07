export interface BundleInfo {
  id: string;
  path: string;
  name: string;
  commands: Record<string, CommandInfo>;
  entry: EntryInfo;
  output: RefInfo;
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
}
