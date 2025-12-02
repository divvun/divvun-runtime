import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "preact/hooks";
import { useTab } from "../contexts/TabContext";
import { useWindow } from "../contexts/WindowContext";
import { BundleInfo, PipelineMetadata, PipelineStep, TabData } from "../types";
import { FluentTester } from "./FluentTester";
import { InputEditor } from "./InputEditor";
import { PipelineOutput } from "./PipelineOutput";

type InternalView = "pipeline" | "fluent";

interface TabContentProps {
  isActive: boolean;
}

export function TabContent({ isActive }: TabContentProps) {
  const { windowId, refreshTabs } = useWindow();
  const { tabId } = useTab();
  const [tabData, setTabData] = useState<TabData | null>(null);
  const [steps, setSteps] = useState<PipelineStep[]>([]);
  const [isRunning, setIsRunning] = useState(false);
  const [isLoading, setIsLoading] = useState(true);
  const [isBundleLoading, setIsBundleLoading] = useState(false);
  const [pipelines, setPipelines] = useState<PipelineMetadata[]>([]);

  // Load tab state from backend ONLY on first mount (not when switching tabs)
  useEffect(() => {
    async function loadTabState() {
      try {
        const data = await invoke<TabData>("get_tab_data", { windowId, tabId });
        setTabData(data);
      } catch (error) {
        console.error("Failed to load tab data:", error);
      } finally {
        setIsLoading(false);
      }
    }

    loadTabState();
  }, []); // Empty dependency array - only run once on mount

  useEffect(() => {
    const unlisten = listen<PipelineStep>("pipeline-step", (event) => {
      // Only handle events for this specific tab
      if (
        event.payload.window_id === windowId && event.payload.tab_id === tabId
      ) {
        setSteps((prev) => [...prev, event.payload]);
      }
    });

    return () => {
      unlisten.then((f) => f());
    };
  }, [windowId, tabId]);

  // Load available pipelines when bundle is loaded
  useEffect(() => {
    async function loadPipelines() {
      if (tabData?.bundle_info) {
        try {
          const pipelineList = await invoke<PipelineMetadata[]>(
            "list_pipelines",
            {
              windowId,
              tabId,
            },
          );
          setPipelines(pipelineList);
        } catch (error) {
          console.error("Failed to load pipelines:", error);
        }
      } else {
        setPipelines([]);
      }
    }

    loadPipelines();
  }, [tabData?.bundle_info?.id]);

  async function openBundle() {
    try {
      const selected = await open({
        multiple: false,
        // filters: [
        //   {
        //     name: "Divvun Runtime Bundle or TypeScript Pipeline",
        //     extensions: ["drb", "ts"],
        //   },
        // ],
      });

      if (selected) {
        setIsBundleLoading(true);
        try {
          const bundleInfo = await invoke<BundleInfo>("load_bundle", {
            windowId,
            tabId,
            path: selected.replace(/^file:\/\//, ""),
            pipelineName: null,
          });
          // Update local state optimistically
          setTabData({ ...tabData!, bundle_info: bundleInfo });
          setSteps([]);
          await refreshTabs();
        } finally {
          setIsBundleLoading(false);
        }
      }
    } catch (error) {
      console.error("Failed to load bundle:", error);
      alert(`Failed to load bundle: ${error}`);
      setIsBundleLoading(false);
    }
  }

  async function handleInputChange(value: string) {
    // Optimistic update
    setTabData({ ...tabData!, pipeline_input: value });
    // Sync to backend (fire and forget)
    invoke("update_tab_input", { windowId, tabId, input: value }).catch(
      console.error,
    );
  }

  async function handleViewChange(view: InternalView) {
    // Optimistic update
    setTabData({ ...tabData!, current_view: view });
    // Sync to backend (fire and forget)
    invoke("update_tab_view", { windowId, tabId, view }).catch(console.error);
  }

  async function handlePipelineChange(e: Event) {
    const select = e.currentTarget as HTMLSelectElement;
    const newPipeline = select.value;

    if (!tabData?.bundle_info) return;

    setIsBundleLoading(true);
    try {
      const bundleInfo = await invoke<BundleInfo>("load_bundle", {
        windowId,
        tabId,
        path: tabData.bundle_info.path,
        pipelineName: newPipeline,
      });
      setTabData({ ...tabData, bundle_info: bundleInfo });
      setSteps([]);
      await refreshTabs();
    } catch (error) {
      console.error("Failed to switch pipeline:", error);
      alert(`Failed to switch pipeline: ${error}`);
    } finally {
      setIsBundleLoading(false);
    }
  }

  async function runPipeline() {
    if (!tabData?.bundle_info || !tabData.pipeline_input) return;

    setIsRunning(true);
    setSteps([]);

    try {
      await invoke("run_pipeline", {
        windowId,
        tabId,
        input: tabData.pipeline_input,
      });
    } catch (error) {
      console.error("Pipeline error:", error);
      alert(`Pipeline error: ${error}`);
    } finally {
      setIsRunning(false);
    }
  }

  // Show loading only on initial mount, not when switching tabs
  if (isLoading) {
    return (
      <div class="tab-content" style={{ display: isActive ? "flex" : "none" }}>
        <div class="loading">Loading tab...</div>
      </div>
    );
  }

  if (!tabData) {
    return null;
  }

  const bundle = tabData.bundle_info;
  const activeView = tabData.current_view as InternalView;

  return (
    <div class="tab-content" style={{ display: isActive ? "flex" : "none" }}>
      <header class="app-header">
        <div class="header-left">
          {bundle
            ? (
              <>
                <span class="bundle-name">
                  {bundle.is_dev_path ? "Dev path:" : "Bundle:"} {bundle.path}
                </span>
                {pipelines.length > 0 && (
                  <select
                    class="pipeline-selector"
                    value={bundle.pipeline_name}
                    onChange={handlePipelineChange}
                    disabled={isBundleLoading}
                  >
                    {pipelines.map((p) => (
                      <option key={p.name} value={p.name}>
                        {p.name}
                        {p.is_default && " [default]"}
                        {p.is_dev && " [dev]"}
                      </option>
                    ))}
                  </select>
                )}
              </>
            )
            : <span class="bundle-name">No bundle loaded</span>}
        </div>
        <div class="header-right">
          <button type="button" onClick={openBundle}>Open Bundle</button>
        </div>
      </header>

      <div class="tabs">
        <button
          type="button"
          class={activeView === "pipeline" ? "tab active" : "tab"}
          onClick={() =>
            handleViewChange("pipeline")}
        >
          Pipeline
        </button>
        <button
          type="button"
          class={activeView === "fluent" ? "tab active" : "tab"}
          onClick={() =>
            handleViewChange("fluent")}
        >
          Fluent Tester
        </button>
      </div>

      <main class="app-main">
        {activeView === "pipeline"
          ? (
            <>
              <div class="output-container">
                <PipelineOutput
                  steps={steps}
                  bundle={bundle}
                  isRunning={isRunning}
                  isBundleLoading={isBundleLoading}
                />
              </div>

              <div class="input-container">
                <InputEditor
                  value={tabData.pipeline_input}
                  onChange={handleInputChange}
                  onRun={runPipeline}
                  disabled={isRunning || !bundle}
                  running={isRunning}
                />
              </div>
            </>
          )
          : (
            <div class="fluent-container">
              <FluentTester windowId={windowId} tabId={tabId} bundle={bundle} />
            </div>
          )}
      </main>
    </div>
  );
}
