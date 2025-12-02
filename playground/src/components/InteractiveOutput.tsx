import { PipelineStep } from "../types";

export type ViewMode = "interactive" | "raw";

interface InteractiveOutputProps {
  step: PipelineStep;
  rawHtml: string;
  viewMode: ViewMode;
}

export function InteractiveOutput(
  { step, rawHtml, viewMode }: InteractiveOutputProps,
) {
  if (!step.event_rich_html) {
    return (
      <div
        class="step-content"
        dangerouslySetInnerHTML={{ __html: rawHtml }}
      />
    );
  }

  return (
    <div class="view-content">
      {viewMode === "interactive"
        ? (
          <div
            dangerouslySetInnerHTML={{ __html: step.event_rich_html }}
          />
        )
        : (
          <div
            class="step-content"
            dangerouslySetInnerHTML={{ __html: rawHtml }}
          />
        )}
    </div>
  );
}
