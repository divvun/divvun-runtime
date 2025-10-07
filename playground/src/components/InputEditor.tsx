interface InputEditorProps {
  value: string;
  onChange: (value: string) => void;
  onRun: () => void;
  disabled: boolean;
  running: boolean;
}

export function InputEditor(
  { value, onChange, onRun, disabled, running }: InputEditorProps,
) {
  const handleKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      onRun();
    }
  };

  return (
    <div class="input-editor">
      {running && <div class="progress-bar"></div>}
      <textarea
        value={value}
        onInput={(e) => onChange(e.currentTarget.value)}
        onKeyDown={handleKeyDown}
        placeholder="Enter input text here..."
        disabled={disabled}
      />
      <button type="button" onClick={onRun} disabled={disabled}>
        {disabled ? "Run" : running ? "Running..." : "Run"}
      </button>
    </div>
  );
}
