import { useEffect, useRef, useState } from "preact/hooks";
import { invoke } from "@tauri-apps/api/core";
import { BundleInfo, ConfigFieldInfo, PipelineStep } from "../types";
import { InteractiveOutput, ViewMode } from "./InteractiveOutput";
import { ConfigEditor } from "./ConfigEditor";

interface PipelineOutputProps {
  steps: PipelineStep[];
  bundle: BundleInfo | null;
  isRunning: boolean;
  isBundleLoading: boolean;
}

export function PipelineOutput(
  { steps, bundle, isRunning, isBundleLoading }: PipelineOutputProps,
) {
  const [expanded, setExpanded] = useState<Record<number, boolean>>({});
  const [allExpanded, setAllExpanded] = useState(true);
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null);
  const [viewModes, setViewModes] = useState<Record<number, ViewMode>>({});
  const [configExpanded, setConfigExpanded] = useState<Record<number, boolean>>(
    {},
  );
  const [configFields, setConfigFields] = useState<
    Record<string, ConfigFieldInfo[] | null>
  >({});
  const [configValues, setConfigValues] = useState<
    Record<number, Record<string, unknown>>
  >({});
  const lastStepRef = useRef<HTMLDivElement>(null);

  // Auto-collapse previous steps when new step arrives and scroll to it
  useEffect(() => {
    if (steps.length > 0) {
      const newExpanded: Record<number, boolean> = {};
      steps.forEach((_, i) => {
        // Only expand the last step
        newExpanded[i] = i === steps.length - 1;
      });
      setExpanded(newExpanded);
      setAllExpanded(false);

      // Scroll to the latest step
      if (lastStepRef.current) {
        lastStepRef.current.scrollIntoView({
          behavior: "smooth",
          block: "nearest",
        });
      }
    }
  }, [steps.length]);

  // Show loading indicator while bundle is loading
  if (isBundleLoading) {
    return (
      <div class="loading-indicator">
        <div class="spinner"></div>
        <p>Loading bundle...</p>
      </div>
    );
  }

  // Show loading indicator while pipeline is running
  if (isRunning && steps.length === 0) {
    return (
      <div class="loading-indicator">
        <div class="spinner"></div>
        <p>Processing pipeline...</p>
      </div>
    );
  }

  // Show appropriate empty state
  if (steps.length === 0) {
    if (!bundle) {
      return (
        <div class="pipeline-output empty">
          <p>Open a bundle to get started</p>
        </div>
      );
    }
    return (
      <div class="pipeline-output empty">
        <p>Enter input text and click Run to process</p>
      </div>
    );
  }

  const toggleStep = (index: number) => {
    setExpanded((prev) => ({
      ...prev,
      [index]: !prev[index],
    }));
  };

  const toggleAll = () => {
    const newState = !allExpanded;
    setAllExpanded(newState);
    const newExpanded: Record<number, boolean> = {};
    steps.forEach((_, i) => {
      newExpanded[i] = newState;
    });
    setExpanded(newExpanded);
  };

  const copyContent = (step: PipelineStep, index: number, e: Event) => {
    e.stopPropagation(); // Prevent toggling the step

    // Strip HTML to get plain text
    const tempDiv = document.createElement("div");
    tempDiv.innerHTML = step.event_html;
    const plainText = tempDiv.textContent || tempDiv.innerText || "";

    // Copy to clipboard
    navigator.clipboard.writeText(plainText).then(() => {
      setCopiedIndex(index);
      setTimeout(() => setCopiedIndex(null), 2000);
    }).catch((err) => {
      console.error("Failed to copy:", err);
    });
  };

  const toggleConfig = async (step: PipelineStep, index: number, e: Event) => {
    e.stopPropagation();

    const key = `${step.command.module}::${step.command.command}`;
    const isExpanding = !configExpanded[index];

    setConfigExpanded((prev) => ({
      ...prev,
      [index]: isExpanding,
    }));

    if (isExpanding && !configFields[key]) {
      try {
        const fields = await invoke<ConfigFieldInfo[] | null>(
          "get_command_config_fields",
          {
            module: step.command.module,
            command: step.command.command,
          },
        );
        setConfigFields((prev) => ({ ...prev, [key]: fields }));
      } catch (err) {
        console.error("Failed to fetch config fields:", err);
      }
    }
  };

  const handleConfigChange = (
    index: number,
    value: Record<string, unknown>,
  ) => {
    setConfigValues((prev) => ({
      ...prev,
      [index]: value,
    }));
  };

  const getCommandConfigName = (step: PipelineStep): string | undefined => {
    if (!bundle) return undefined;
    const cmdInfo = Object.values(bundle.commands).find(
      (cmd) =>
        cmd.module === step.command.module &&
        cmd.command === step.command.command,
    );
    return cmdInfo?.config_name;
  };

  return (
    <div class="pipeline-output">
      <div class="output-controls">
        <button onClick={toggleAll} class="toggle-all">
          {allExpanded ? "Collapse All" : "Expand All"}
        </button>
      </div>
      {steps.map((step, i) => {
        const isExpanded = expanded[i] !== undefined ? expanded[i] : true;
        const isLastStep = i === steps.length - 1;
        return (
          <div
            key={i}
            class="pipeline-step"
            ref={isLastStep ? lastStepRef : null}
          >
            <div class="step-header" onClick={() => toggleStep(i)}>
              <span class="step-toggle">{isExpanded ? "▼" : "▶"}</span>
              <span class="step-index">[{i}]</span>
              <span class="step-command">{step.command_key}</span>
              <span class="step-display">
                {step.command.module}::{step.command.command}
                {step.command.id && ` (${step.command.id})`}
              </span>
              <button
                type="button"
                class="toggle-btn"
                onClick={(e) => copyContent(step, i, e)}
                title="Copy content"
              >
                {copiedIndex === i ? "Copied" : "Copy"}
              </button>
            </div>
            {isExpanded && (
              <>
                <div class="step-params">
                  <span>{step.command_display}</span>
                  {step.event_rich_html && (
                    <div class="header-view-toggle">
                      <button
                        type="button"
                        class={viewModes[i] === "raw"
                          ? "toggle-btn"
                          : "toggle-btn active"}
                        onClick={() =>
                          setViewModes({ ...viewModes, [i]: "interactive" })}
                      >
                        Interactive
                      </button>
                      <button
                        type="button"
                        class={viewModes[i] === "raw"
                          ? "toggle-btn active"
                          : "toggle-btn"}
                        onClick={() =>
                          setViewModes({ ...viewModes, [i]: "raw" })}
                      >
                        Raw
                      </button>
                    </div>
                  )}
                </div>
                <InteractiveOutput
                  step={step}
                  rawHtml={step.event_html}
                  viewMode={viewModes[i] || "interactive"}
                />
                {getCommandConfigName(step) && (
                  <div class="config-section">
                    <div
                      class="config-header"
                      onClick={(e) => toggleConfig(step, i, e)}
                    >
                      <span class="config-toggle">
                        {configExpanded[i] ? "▼" : "▶"}
                      </span>
                      <span class="config-label">Configuration</span>
                    </div>
                    {configExpanded[i] && (() => {
                      const key =
                        `${step.command.module}::${step.command.command}`;
                      const fields = configFields[key];
                      if (fields === undefined) {
                        return <div class="config-loading">Loading...</div>;
                      }
                      if (fields === null) {
                        return (
                          <div class="config-none">
                            No configuration available
                          </div>
                        );
                      }
                      return (
                        <ConfigEditor
                          fields={fields}
                          value={configValues[i] || {}}
                          onChange={(value) => handleConfigChange(i, value)}
                        />
                      );
                    })()}
                  </div>
                )}
              </>
            )}
          </div>
        );
      })}
    </div>
  );
}
