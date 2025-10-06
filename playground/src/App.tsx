import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "preact/hooks";
import "./App.css";
import { InputEditor } from "./components/InputEditor";
import { PipelineOutput } from "./components/PipelineOutput";
import { BundleInfo, PipelineStep } from "./types";

function App() {
  const [bundle, setBundle] = useState<BundleInfo | null>(null);
  const [input, setInput] = useState("");
  const [steps, setSteps] = useState<PipelineStep[]>([]);
  const [isRunning, setIsRunning] = useState(false);

  useEffect(() => {
    const unlisten = listen<PipelineStep>("pipeline-step", (event) => {
      setSteps((prev) => [...prev, event.payload]);
    });

    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  async function openBundle() {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: "Divvun Runtime Bundle",
            extensions: ["drb"],
          },
        ],
      });

      if (selected) {
        const bundleInfo = await invoke<BundleInfo>("load_bundle", {
          path: selected,
        });
        setBundle(bundleInfo);
        setSteps([]);
      }
    } catch (error) {
      console.error("Failed to load bundle:", error);
      alert(`Failed to load bundle: ${error}`);
    }
  }

  async function runPipeline() {
    if (!bundle || !input) return;

    setIsRunning(true);
    setSteps([]);

    try {
      await invoke("run_pipeline", {
        bundleId: bundle.id,
        input: input,
      });
    } catch (error) {
      console.error("Pipeline error:", error);
      alert(`Pipeline error: ${error}`);
    } finally {
      setIsRunning(false);
    }
  }

  return (
    <div class="app">
      <header class="app-header">
        <div class="header-left">
          {bundle ? (
            <span class="bundle-name">File: {bundle.name}</span>
          ) : (
            <span class="bundle-name">No bundle loaded</span>
          )}
        </div>
        <div class="header-right">
          <button type="button" onClick={openBundle}>Open Bundle</button>
        </div>
      </header>

      <main class="app-main">
        <div class="output-container">
          <PipelineOutput steps={steps} bundle={bundle} isRunning={isRunning} />
        </div>

        <div class="input-container">
          <InputEditor
            value={input}
            onChange={setInput}
            onRun={runPipeline}
            disabled={isRunning || !bundle}
            running={isRunning}
          />
        </div>
      </main>
    </div>
  );
}

export default App;
