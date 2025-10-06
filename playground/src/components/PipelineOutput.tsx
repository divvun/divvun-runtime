import { useState, useEffect, useRef } from "preact/hooks";
import { PipelineStep, BundleInfo } from "../types";

interface PipelineOutputProps {
  steps: PipelineStep[];
  bundle: BundleInfo | null;
  isRunning: boolean;
}

export function PipelineOutput({ steps, bundle, isRunning }: PipelineOutputProps) {
  const [expanded, setExpanded] = useState<Record<number, boolean>>({});
  const [allExpanded, setAllExpanded] = useState(true);
  const [copiedIndex, setCopiedIndex] = useState<number | null>(null);
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
        lastStepRef.current.scrollIntoView({ behavior: "smooth", block: "nearest" });
      }
    }
  }, [steps.length]);

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
    const tempDiv = document.createElement('div');
    tempDiv.innerHTML = step.event_html;
    const plainText = tempDiv.textContent || tempDiv.innerText || '';

    // Copy to clipboard
    navigator.clipboard.writeText(plainText).then(() => {
      setCopiedIndex(index);
      setTimeout(() => setCopiedIndex(null), 2000);
    }).catch((err) => {
      console.error('Failed to copy:', err);
    });
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
                class="copy-button"
                onClick={(e) => copyContent(step, i, e)}
                title="Copy content"
              >
                {copiedIndex === i ? "✓" : "📋"}
              </button>
            </div>
            {isExpanded && (
              <div
                class="step-content"
                dangerouslySetInnerHTML={{ __html: step.event_html }}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}
