import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "preact/hooks";

interface FluentTesterProps {
  bundleId: string | null;
}

interface FluentFileInfo {
  path: string;
  locale: string;
}

interface FluentMessageInfo {
  id: string;
  has_desc: boolean;
  detected_params: string[];
}

interface FluentMessageResult {
  title: string;
  description: string;
}

export function FluentTester({ bundleId }: FluentTesterProps) {
  const [files, setFiles] = useState<FluentFileInfo[]>([]);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [messages, setMessages] = useState<FluentMessageInfo[]>([]);
  const [selectedMessage, setSelectedMessage] = useState<string | null>(null);
  const [selectedMessageInfo, setSelectedMessageInfo] = useState<
    FluentMessageInfo | null
  >(null);
  const [locale, setLocale] = useState<string>("en");
  const [args, setArgs] = useState<Record<string, string>>({});
  const [result, setResult] = useState<FluentMessageResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (bundleId) {
      loadFiles();
    } else {
      setFiles([]);
      setSelectedFile(null);
      setMessages([]);
      setSelectedMessage(null);
      setResult(null);
    }
  }, [bundleId]);

  useEffect(() => {
    if (selectedFile && bundleId) {
      loadMessages();
    } else {
      setMessages([]);
      setSelectedMessage(null);
      setResult(null);
    }
  }, [selectedFile, bundleId]);

  useEffect(() => {
    if (selectedMessage) {
      const msgInfo = messages.find((m) => m.id === selectedMessage);
      setSelectedMessageInfo(msgInfo || null);

      if (msgInfo) {
        const newArgs: Record<string, string> = {};
        for (const param of msgInfo.detected_params) {
          newArgs[param] = args[param] || "";
        }
        setArgs(newArgs);
      }
    } else {
      setSelectedMessageInfo(null);
      setArgs({});
    }
  }, [selectedMessage, messages]);

  useEffect(() => {
    if (selectedMessage && bundleId) {
      testMessage();
    }
  }, [selectedMessage, locale, bundleId]);

  const loadFiles = async () => {
    if (!bundleId) return;

    setLoading(true);
    setError(null);
    try {
      const ftlFiles = await invoke<FluentFileInfo[]>("list_ftl_files", {
        bundleId,
      });
      setFiles(ftlFiles);

      if (ftlFiles.length > 0) {
        setSelectedFile(ftlFiles[0].path);
        const fileLocale = ftlFiles[0].locale;
        setLocale(fileLocale);
      }
    } catch (e) {
      setError(`Failed to load .ftl files: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  const loadMessages = async () => {
    if (!bundleId || !selectedFile) return;

    setLoading(true);
    setError(null);
    try {
      const msgs = await invoke<FluentMessageInfo[]>("get_ftl_messages", {
        bundleId,
        filePath: selectedFile,
      });
      setMessages(msgs);

      if (msgs.length > 0) {
        setSelectedMessage(msgs[0].id);
      }
    } catch (e) {
      setError(`Failed to load messages: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  const testMessage = async (argsToUse = args) => {
    if (!bundleId || !selectedMessage) return;

    setLoading(true);
    setError(null);
    try {
      const res = await invoke<FluentMessageResult>("test_ftl_message", {
        bundleId,
        locale,
        messageId: selectedMessage,
        args: argsToUse,
      });
      setResult(res);
    } catch (e) {
      setError(`Failed to format message: ${e}`);
      setResult(null);
    } finally {
      setLoading(false);
    }
  };

  const handleArgChange = (param: string, value: string) => {
    const newArgs = { ...args, [param]: value };
    setArgs(newArgs);
    testMessage(newArgs);
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
  };

  if (!bundleId) {
    return (
      <div class="fluent-tester">
        <div class="fluent-placeholder">
          Load a bundle to test Fluent messages
        </div>
      </div>
    );
  }

  if (files.length === 0) {
    return (
      <div class="fluent-tester">
        <div class="fluent-placeholder">
          No .ftl files found in bundle
        </div>
      </div>
    );
  }

  return (
    <div class="fluent-tester">
      <div class="fluent-controls">
        <div class="control-group">
          <label>File:</label>
          <select
            value={selectedFile || ""}
            onChange={(e) => setSelectedFile(e.currentTarget.value)}
          >
            {files.map((f) => (
              <option key={f.path} value={f.path}>
                {f.path} ({f.locale})
              </option>
            ))}
          </select>
        </div>

        <div class="control-group">
          <label>Locale:</label>
          <input
            type="text"
            value={locale}
            onInput={(e) => setLocale(e.currentTarget.value)}
            placeholder="en"
          />
        </div>

        <div class="control-group">
          <label>Message:</label>
          <select
            value={selectedMessage || ""}
            onChange={(e) => setSelectedMessage(e.currentTarget.value)}
          >
            {messages.map((m) => (
              <option key={m.id} value={m.id}>
                {m.id}
              </option>
            ))}
          </select>
        </div>

        {selectedMessageInfo &&
          selectedMessageInfo.detected_params.length > 0 && (
          <div class="arguments-section">
            <h3>Arguments</h3>
            {selectedMessageInfo.detected_params.map((param) => (
              <div key={param} class="control-group">
                <label>${param}:</label>
                <input
                  type="text"
                  value={args[param] || ""}
                  onInput={(e) =>
                    handleArgChange(param, e.currentTarget.value)}
                  placeholder={`Value for ${param}`}
                />
              </div>
            ))}
          </div>
        )}
      </div>

      {error && <div class="fluent-error">{error}</div>}

      {result && (
        <div class="fluent-result">
          <div class="result-section">
            <div class="result-header">
              <h3>Title</h3>
              <button
                type="button"
                class="copy-btn"
                onClick={() => copyToClipboard(result.title)}
              >
                📋
              </button>
            </div>
            <div class="result-content">{result.title}</div>
          </div>

          <div class="result-section">
            <div class="result-header">
              <h3>Description</h3>
              <button
                type="button"
                class="copy-btn"
                onClick={() => copyToClipboard(result.description)}
              >
                📋
              </button>
            </div>
            <div class="result-content">{result.description}</div>
          </div>
        </div>
      )}

      {loading && <div class="fluent-loading">Loading...</div>}
    </div>
  );
}
