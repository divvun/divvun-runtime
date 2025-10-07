# Divvun Runtime Playground - Implementation Plan

## Overview

The Divvun Runtime Playground is a desktop GUI application built with Tauri that
provides an interactive interface for running and debugging linguistic
pipelines. It serves as a companion to the CLI, offering visual pipeline
execution, step-by-step output inspection, and syntax highlighting for various
data formats.

## Implementation Status

### ✅ Completed Features

**Phase 1: Core Functionality**

- ✅ Bundle loading (.drb files and directories)
- ✅ File dialog integration
- ✅ Bundle info display in header
- ✅ Basic pipeline execution with text input
- ✅ Run/stop controls

**Phase 2: Enhanced Output**

- ✅ Pipeline step capture via tap function
- ✅ Streaming steps via Tauri events
- ✅ Collapsible/expandable steps
- ✅ Auto-collapse previous steps (only latest expanded)
- ✅ Auto-scroll to latest step
- ✅ Syntax highlighting (syntect backend, not CodeMirror)
- ✅ CG3, JSON, and plain text highlighting
- ✅ Command display with full argument formatting
- ✅ Copy button for each step output
- ✅ Toggle all expand/collapse

**Phase 3: UX Enhancements**

- ✅ Loading spinner before first event
- ✅ Progress bar in input area during execution
- ✅ Different empty states based on context
- ✅ Dark theme with VSCode-inspired colors
- ✅ Iosevka Web font bundled
- ✅ Command kind field enrichment from metadata

### Key Implementation Differences from Original Plan

1. **Syntax Highlighting**: Used syntect (Rust) instead of CodeMirror
   (JavaScript)
   - Backend generates HTML with syntax highlighting
   - Sublime syntax definitions for extensibility
   - Better performance and consistency with CLI formatting

2. **UI Layout**: Streamlined input area
   - Removed input header, button floats in bottom-right
   - Reduced input height to ~100px
   - Progress bar integrated at top of input area

3. **Command Formatting**: Direct Rust formatting
   - `Command::as_str(ansi: bool)` method formats entire command
   - `Value::as_str(ansi: bool)` handles nested values
   - Backend sends pre-formatted `command_display` string
   - No client-side formatting needed

4. **Auto-behaviors**: Enhanced UX
   - Auto-collapse all previous steps when new step arrives
   - Auto-scroll to keep latest step visible
   - Only last step expanded by default

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

## UI Layout (Actual Implementation)

```
┌─────────────────────────────────────────────────────────┐
│ File: example.drb                   [Expand All] [Open]  │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  Pipeline Output (scrollable)                            │
│  ┌────────────────────────────────────────────────────┐ │
│  │ ▶ [0] command_key                         [📋]     │ │
│  │     module::command(arg=<type>val) -> returns      │ │
│  │                                                    │ │ │
│  │ ▼ [1] command_key                         [📋]     │ │
│  │     module::command(arg=<type>val) -> returns      │ │
│  │   ┌──────────────────────────────────────────────┐ │ │
│  │   │ Syntax-highlighted output                     │ │ │
│  │   │ (CG3, JSON, or plain text)                    │ │ │
│  │   └──────────────────────────────────────────────┘ │ │
│  └────────────────────────────────────────────────────┘ │
│                                                          │
├─────────────────────────────────────────────────────────┤
│  [Progress bar when running]                             │
│  ┌────────────────────────────────────────────────────┐ │
│  │ Enter input text here...                [Run]      │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

**Key Layout Features:**

- ✅ Header shows bundle name and controls
- ✅ Output area takes most of screen (flex: 1)
- ✅ Input area is compact (~100px) at bottom
- ✅ No input header, button floats in bottom-right corner
- ✅ Progress bar appears at top of input area when pipeline is running
- ✅ Auto-scroll keeps latest step visible
- ✅ Step headers are clickable to toggle expand/collapse
- ✅ Copy button in each step header

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

**Streaming Results:** Use Tauri events to stream pipeline step outputs:

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

- ✅ Collapsible/expandable output for each pipeline step
- ✅ Step header format: `[index] command_key`
- ✅ Step subheader shows full command:
  `module::command(arg1 = <type>value, ...) -> returns`
- ✅ Syntax highlighting based on **kind** field:
  - **cg3**: Constraint Grammar format with custom Sublime syntax
  - **json**: JSON syntax highlighting
  - **plain**: HTML-escaped plain text
- ✅ Auto-collapse previous steps when new step arrives (only latest expanded)
- ✅ Auto-scroll to keep latest step visible
- ✅ Show/hide individual steps via expand/collapse arrow
- ✅ Expand/collapse all button in header
- ✅ Copy button for each step (📋 icon, shows ✓ on success)
- ✅ Loading spinner with "Processing pipeline..." message before first event
- ✅ Different empty states:
  - No bundle: "Open a bundle to get started"
  - Bundle loaded: "Enter input text and click Run to process"

**Command Display Format:** Each expanded step shows the full command above the
output:

```
module::command(arg1 = <type>value, arg2 = <type>value) -> returns
```

Arguments are formatted using `Value::as_str(ansi: false)` which produces clean
output:

- Strings: `"hello"`
- Numbers: `42`
- Maps: `{key: value, key2: value2}`
- Arrays: `[item1, item2]`
- No ANSI escape codes (unlike CLI which uses colors)

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
</PipelineOutput>;
```

### 4. Syntax Highlighting

**Implementation: Syntect (Rust Backend)**

Instead of CodeMirror in the frontend, syntax highlighting is performed on the
backend using the syntect library. This approach provides:

- Consistent formatting with CLI output
- Better performance (no large JS bundles)
- Extensible via Sublime syntax definitions
- HTML generation with inline styles

**Backend Implementation:**

```rust
// src-tauri/src/syntax.rs
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::{SyntaxDefinition, SyntaxSet, SyntaxSetBuilder};

// Custom syntax set with CG3
static CUSTOM_SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(|| {
    let mut builder = SyntaxSetBuilder::new();
    let cg3_syntax = include_str!("../syntaxes/cg3.sublime-syntax");
    builder.add(
        SyntaxDefinition::load_from_str(cg3_syntax, true, Some("cg3"))
            .expect("Failed to load CG3 syntax"),
    );
    builder.build()
});

pub fn highlight_to_html(content: &str, syntax_name: &str) -> String {
    // Returns HTML with syntax highlighting
}
```

**CG3 Sublime Syntax:** Custom syntax definition in YAML format
(`syntaxes/cg3.sublime-syntax`):

- Surface forms: `"<word>"`
- Base forms with postfix: `"lemma"S`, `"lemma"phon`
- Tags: `N`, `Sg`, `Gen`, etc.
- Mapping tags: `@func`, `@subj`
- Weight tags: `<W:0.0>`, `<WA:15.3>`
- Literal lines: lines starting with `:`
- Comment lines: lines starting with `;`
- Special highlighting for `<`, `>`, `/` characters

**Determining the Kind Field:** The backend determines syntax kind via:

1. CommandDef metadata (compile-time): `kind: Some("cg3")` in module definitions
2. Runtime enrichment: Commands inherit `kind` from CommandDef if not set in AST
3. Content detection: JSON data automatically gets `kind: "json"`

**Frontend Display:** The frontend receives pre-highlighted HTML and displays it
directly:

```tsx
<div
  class="step-content"
  dangerouslySetInnerHTML={{ __html: step.event_html }}
/>;
```

### 5. Theme & Styling

**✅ Implemented Dark Theme**

- Always dark mode (no light theme toggle)
- VSCode-inspired color scheme:
  - Background: `#1e1e1e`
  - Panels: `#252526`
  - Borders: `#3e3e42`
  - Text: `#f6f6f6` / `#cccccc`
  - Accent: `#4fc1ff` (cyan/blue)
  - Muted: `#999` / `#888`

**✅ Typography**

- Iosevka Web font bundled (WOFF2 format)
- Font ligatures and variations enabled:
  ```css
  font-feature-settings: "liga", "calt";
  font-variation-settings: normal;
  ```
- Monospace used for:
  - Step headers
  - Command display
  - Step content
  - Input textarea
- System font for UI chrome (header, buttons)

**✅ Layout & Spacing**

- Tab width: 4 spaces
- Compact padding throughout
- Input area: 100px fixed height
- Smooth transitions on hover/interactions

### 6. Command Formatting Architecture

**Backend: `Command::as_str(ansi: bool)` and `Value::as_str(ansi: bool)`**

The command formatting system properly handles ANSI escape codes:

```rust
// src/ast/mod.rs
impl Command {
    pub fn as_str(&self, ansi: bool) -> String {
        // Formats: module::command(args...) -> returns
        // Uses Value::as_str(ansi) for argument values
    }
}

impl Value {
    pub fn as_str(&self, ansi: bool) -> String {
        // Formats values based on type:
        // - Int: plain number or colored
        // - String: quoted string, plain or colored
        // - Map: {key: value, ...} with or without colors
        // - Array: [item1, item2] with or without colors
    }
}
```

**How it works:**

1. CLI uses `format!("{}", command)` → `Display` trait → `as_str(true)` → ANSI
   codes included
2. Playground backend calls `cmd.as_str(false)` → no ANSI codes
3. Backend sends `command_display: String` to frontend
4. Frontend displays pre-formatted string directly (no parsing needed)

**Benefits:**

- Single source of truth for formatting logic
- No duplication between CLI and playground
- Frontend receives clean, ready-to-display strings
- Complex nested values (Maps, Arrays) formatted correctly

### 7. Advanced Features (Future)

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
  | "Finish"
  | "Close";

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

✅ **Implemented:**

```rust
// Bundles stored in Arc<Mutex<HashMap<String, Bundle>>>
// UUID-based IDs for tracking bundles across Tauri calls
// Bundles persist for app lifetime (no explicit cleanup yet)
```

### Streaming Performance

✅ **Implemented:**

- Tauri events for streaming (not polling)
- Auto-collapse previous steps to reduce DOM size
- Auto-scroll with smooth behavior
- Steps rendered as HTML (no re-rendering on highlight changes)

**Future optimizations:**

- Virtualize long output lists if needed (react-window)
- Limit step history (configurable)
- Debounce rapid step updates if needed

### Command Formatting Architecture

✅ **Implemented:**

- `Command::as_str(ansi: bool)` method in Rust
- `Value::as_str(ansi: bool)` method for nested values
- Backend sends pre-formatted `command_display` string
- Frontend displays directly with no parsing
- Single source of truth for formatting logic
- CLI and playground use same formatting code

### Syntax Highlighting Architecture

✅ **Implemented:**

- Backend-side highlighting with syntect
- Sublime syntax definitions for extensibility
- HTML generation with inline styles
- Custom CG3 syntax definition
- Kind field enrichment from CommandDef metadata
- Frontend receives pre-highlighted HTML

### Error Boundaries

**Partially implemented:**

- Rust errors caught and displayed as alerts
- Pipeline errors logged to console

**Future improvements:**

- Better error UI (inline in output)
- Error recovery options
- Error step highlighting

### Cross-Platform

**Considerations:**

- File dialogs work cross-platform (via Tauri)
- Path handling via std::path (cross-platform)
- Keyboard shortcuts: Cmd+Enter (Mac) / Ctrl+Enter (Windows/Linux)

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

## Dependencies

**Frontend** (`package.json`):

```json
{
  "dependencies": {
    "@tauri-apps/api": "^2",
    "@tauri-apps/plugin-dialog": "^2",
    "preact": "^10.25.1"
  }
}
```

**Backend** (`src-tauri/Cargo.toml`):

```toml
[dependencies]
tauri = { version = "2", features = ["devtools"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
anyhow = "1"
uuid = { version = "1", features = ["v4"] }
futures-util = "0.3"

# Syntax highlighting
syntect = "5"
once_cell = "1"
html-escape = "0.2"

# Workspace dependency
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
