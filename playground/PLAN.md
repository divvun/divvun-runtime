# Divvun Runtime Playground - Implementation Plan

## Overview

The Divvun Runtime Playground is a desktop GUI application built with Tauri that provides an interactive interface for running and debugging linguistic pipelines. It serves as a companion to the CLI, offering visual pipeline execution, step-by-step output inspection, and syntax highlighting for various data formats.

## Architecture

### Technology Stack

- **Frontend**: Preact + TypeScript + Vite
- **Backend**: Rust (Tauri) with direct integration to `divvun-runtime` library
- **Styling**: CSS Modules or Tailwind CSS
- **Syntax Highlighting**: CodeMirror 6 with custom language extensions
- **State Management**: Preact signals for local state

### Core Integration Pattern

The playground will use the same Bundle API as the CLI:

```rust
// Load bundles
let bundle = Bundle::from_bundle(path)?;  // .drb files
let bundle = Bundle::from_path(path)?;    // Directory with ast.json

// Create pipeline with tap function to intercept steps
let mut pipe = bundle.create_with_tap(config, tap).await?;

// Execute with streaming results
let mut stream = pipe.forward(Input::String(input)).await;
while let Some(result) = stream.next().await {
    // Process each output
}
```

## UI Layout

```
┌─────────────────────────────────────────────────────────┐
│ File: example.drb                    [Expand All] [...]  │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  Pipeline Output                                         │
│  ┌────────────────────────────────────────────────────┐ │
│  │ ▼ [0] entry → spell::check                         │ │
│  │   ┌──────────────────────────────────────────────┐ │ │
│  │   │ "hello world"                                 │ │ │
│  │   └──────────────────────────────────────────────┘ │ │
│  │                                                    │ │ │
│  │ ▶ [1] spell::check → output                        │ │
│  │                                                    │ │ │
│  │ ▼ [2] output (json)                                │ │ │
│  │   ┌──────────────────────────────────────────────┐ │ │
│  │   │ {                                             │ │ │
│  │   │   "suggestions": [...],                       │ │ │
│  │   │   ...                                         │ │ │
│  │   │ }                                             │ │ │
│  │   └──────────────────────────────────────────────┘ │ │
│  └────────────────────────────────────────────────────┘ │
│                                                          │
├─────────────────────────────────────────────────────────┤
│  Input                                  [Run] [Stop]     │
│  ┌────────────────────────────────────────────────────┐ │
│  │ Hello world                                        │ │
│  │                                                    │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

## Core Features

### 1. Bundle Management

**Features:**
- Open bundle files (.drb) or directories via File menu
- Display current bundle name in header
- Menu options for viewing bundle info (AST, commands, metadata)

**Tauri Commands:**
```rust
#[tauri::command]
async fn load_bundle(path: String) -> Result<BundleInfo, String>
```

### 2. Pipeline Execution

**Features:**
- Text input area at bottom of window for string input
- Run button to execute pipeline
- Stop button to cancel execution
- Initially supports only `Input::String` type (other types can be added later)

**Tauri Commands:**
```rust
#[tauri::command]
async fn run_pipeline(
    bundle_id: String,
    input: String,
    config: serde_json::Value,
) -> Result<String, String>

#[tauri::command]
async fn stop_pipeline(execution_id: String) -> Result<(), String>
```

**Streaming Results:**
Use Tauri events to stream pipeline step outputs:
```rust
// Backend emits events with optional "kind" field for syntax highlighting
app_handle.emit_all("pipeline-step", StepOutput {
    execution_id,
    step_index,
    command_key,
    command_display,
    input_event,
    kind: Some("cg3"), // or "plain", "json", etc.
})?;

// Frontend listens
useEffect(() => {
  const unlisten = listen('pipeline-step', (event) => {
    setPipelineSteps(prev => [...prev, event.payload]);
  });
  return () => { unlisten.then(f => f()); };
}, []);
```

### 3. Output Visualization

**Features:**
- Collapsible/expandable output for each pipeline step
- Step format: `[key] module::command(args...) -> return_type`
- Syntax highlighting based on **kind** field:
  - **cg3**: Constraint Grammar format
  - **plain**: Plain text with line numbers
  - **json**: JSON syntax highlighting
  - More kinds can be added as needed
- Show/hide individual steps via expand/collapse arrow
- Expand/collapse all controls in header
- Copy individual step outputs (future)

**Component Structure:**
```tsx
<PipelineOutput>
  {steps.map((step, i) => (
    <PipelineStep
      key={i}
      index={i}
      command={step.command}
      input={step.input}
      kind={step.kind}
      expanded={expandedSteps[i]}
      onToggle={() => toggleStep(i)}
    >
      <SyntaxHighlighter
        kind={step.kind ?? "plain"}
        content={formatContent(step.input)}
      />
    </PipelineStep>
  ))}
</PipelineOutput>
```

### 4. Syntax Highlighting

**Implementation: CodeMirror 6**
- Lightweight (~500KB)
- Extensible with custom language packages
- Good performance for large outputs
- Can create custom syntax highlighters for CG3 and other linguistic formats

**Usage:**
```tsx
import CodeMirror from '@uiw/react-codemirror';
import { json } from '@codemirror/lang-json';
import { cg3Language } from './languages/cg3'; // Custom language

function getLanguageExtension(kind: string | null) {
  switch (kind) {
    case 'json': return json();
    case 'cg3': return cg3Language();
    case 'plain':
    default: return [];
  }
}

<CodeMirror
  value={content}
  extensions={[getLanguageExtension(kind)]}
  editable={false}
  basicSetup={{
    lineNumbers: true,
    highlightActiveLineGutter: false,
    highlightActiveLine: false,
  }}
/>
```

**Custom Language Support:**
Custom CodeMirror language extensions can be created for linguistic formats like CG3. This involves defining tokenizers and syntax highlighting rules using the `@codemirror/language` package.

**Determining the Kind Field:**
The backend tap function should set the `kind` field based on the module/command or data format:
- CG3 module outputs → `kind: "cg3"`
- JSON data → `kind: "json"`
- Plain text → `kind: "plain"` or no kind field (default)
- Custom formats can define their own kinds and corresponding CodeMirror extensions

### 5. Advanced Features (Phase 2+)

- **Stepping Mode**: Step through pipeline one command at a time
- **Breakpoints**: Pause execution at specific commands
- **Export to Markdown**: Generate debug reports (like CLI `:save`)
- **Pipeline Visualization**: Graph view of command flow
- **Input History**: Save and replay previous inputs
- **Diff View**: Compare outputs between runs
- **Performance Metrics**: Show execution time per step

## Data Models

### Frontend Types

```typescript
interface BundleInfo {
  id: string;
  path: string;
  name: string;
  commands: Record<string, CommandInfo>;
  entry: { value_type: string };
  output: { ref: string };
}

interface CommandInfo {
  module: string;
  command: string;
  args: Record<string, ArgInfo>;
  input: { Single?: string } | { Multiple?: string[] };
  returns: string;
}

interface ArgInfo {
  type: string;
  value_type?: string;
  value?: any;
}

interface PipelineStep {
  executionId: string;
  stepIndex: number;
  commandKey: string;
  commandDisplay: string;
  event: InputEvent;
  kind?: string; // Syntax highlighting kind: "cg3", "plain", "json", etc.
  timestamp: number;
}

type InputEvent =
  | { Input: InputData }
  | { Error: string }
  | 'Finish'
  | 'Close';

type InputData =
  | { String: string }
  | { Bytes: number[] }
  | { Json: any }
  | { ArrayString: string[] }
  | { ArrayBytes: number[][] }
  | { Multiple: InputData[] };
```

### Backend State Management

```rust
// Global state to track bundles and executions
struct PlaygroundState {
    bundles: Arc<RwLock<HashMap<String, Bundle>>>,
    executions: Arc<RwLock<HashMap<String, ExecutionHandle>>>,
}

struct ExecutionHandle {
    handle: PipelineHandle,
    cancel_token: CancellationToken,
}
```

## Implementation Roadmap

### Phase 1: Core Functionality (MVP)

1. **Bundle Loading**
   - Tauri command to load .drb bundles
   - Display bundle name in header
   - File menu for opening bundles

2. **Basic Pipeline Execution**
   - Text input area at bottom
   - Run button triggering Tauri command
   - Display final output (no stepping yet)

3. **Simple Output Display**
   - Show final result with basic formatting
   - Plain text display initially (no syntax highlighting)

### Phase 2: Enhanced Output

1. **Pipeline Step Display**
   - Implement tap function to capture each step
   - Include "kind" field in step output for syntax type
   - Stream steps via Tauri events
   - Display collapsible steps in UI

2. **Syntax Highlighting with CodeMirror**
   - Integrate CodeMirror 6
   - Use "kind" field to select appropriate language extension
   - Support plain, json initially
   - Add CG3 syntax highlighter

3. **Show/Hide Controls**
   - Toggle individual steps
   - Expand/collapse all button in header

### Phase 3: Configuration & Debugging

1. **Config Support**
   - Modal or panel for editing command configs (JSON)
   - Pass configs to pipeline execution
   - Save/load config presets (future)

2. **Error Handling**
   - Display pipeline errors inline in output
   - Highlight error step in red
   - Show error details

3. **Export Features**
   - Export to markdown (like CLI `:save`)
   - Copy individual step outputs to clipboard

### Phase 4: Advanced Features

1. **Stepping & Breakpoints**
   - Step through pipeline command-by-command
   - Set breakpoints on commands
   - Resume/continue controls

2. **Multiple Bundles**
   - Switch between loaded bundles
   - Compare different pipelines
   - Bundle management UI

3. **Input Management**
   - Support Bytes input (file upload)
   - JSON input with editor
   - Array inputs with UI
   - Input history and replay

### Phase 5: Polish & UX

1. **Visualization**
   - Pipeline graph view (Mermaid or D3)
   - Visual command flow
   - Click to jump to step output

2. **Performance**
   - Show execution time per step
   - Progress indicators
   - Optimize for large outputs

3. **Settings & Preferences**
   - Theme selection (light/dark)
   - Editor settings
   - Window layout persistence

## Technical Considerations

### Bundle Lifecycle

```rust
// Keep bundles loaded in memory
// Use unique IDs to reference them across Tauri calls
// Clean up on window close or explicit unload
```

### Streaming Performance

- Use Tauri events for streaming (not polling)
- Debounce rapid step updates
- Virtualize long output lists (react-window)
- Limit step history (configurable)

### Error Boundaries

- Catch and display Rust errors gracefully
- Show user-friendly error messages
- Provide error recovery options (retry, reload)

### Cross-Platform

- Test file dialogs on all platforms
- Handle path separators correctly
- Consider platform-specific shortcuts

## Development Commands

```bash
# Install dependencies
npm install
npm install @uiw/react-codemirror @codemirror/lang-json @codemirror/language

# Run in dev mode
npm run tauri dev

# Build for production
npm run tauri build
```

## Dependencies to Add

```json
{
  "dependencies": {
    "@uiw/react-codemirror": "^4.23.0",
    "@codemirror/lang-json": "^6.0.0",
    "@codemirror/language": "^6.0.0",
    "@tauri-apps/api": "^2",
    "preact": "^10.25.1"
  }
}
```

```toml
# src-tauri/Cargo.toml
[dependencies]
tauri = { version = "2", features = ["devtools"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
anyhow = "1"
uuid = { version = "1", features = ["v4"] }

# Add workspace dependency
divvun-runtime = { path = "../.." }
```

## Testing Strategy

1. **Unit Tests**: Test Tauri commands in isolation
2. **Integration Tests**: Test full pipeline execution
3. **UI Tests**: Test component rendering and interactions
4. **Manual Tests**: Test with real bundles and pipelines

## References

- CLI REPL implementation: `cli/src/command/run.rs`
- Bundle API: `src/bundle.rs`
- Pipeline execution: `src/ast/mod.rs`
- Tap function pattern: `cli/src/command/run.rs:114-158`
- Input/Output types: `src/modules/mod.rs:68-87`
