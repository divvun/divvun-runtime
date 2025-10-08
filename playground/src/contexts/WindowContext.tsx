import { createContext } from "preact";
import { useContext, useEffect, useState } from "preact/hooks";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { TabInfo, WindowStateInfo } from "../types";

interface WindowContextValue {
  windowId: string;
  tabs: TabInfo[];
  activeTabIndex: number;
  createTab: () => Promise<void>;
  closeTab: (tabId: string) => Promise<void>;
  switchTab: (index: number) => Promise<void>;
  duplicateTab: (tabId: string) => Promise<void>;
  refreshTabs: () => Promise<void>;
}

const WindowContext = createContext<WindowContextValue | null>(null);

export function WindowProvider(
  { children }: { children: preact.ComponentChildren },
) {
  const [windowId, setWindowId] = useState<string>("");
  const [tabs, setTabs] = useState<TabInfo[]>([]);
  const [activeTabIndex, setActiveTabIndex] = useState(0);

  useEffect(() => {
    async function init() {
      const window = getCurrentWindow();
      const label = window.label;
      setWindowId(label);

      try {
        const state = await invoke<WindowStateInfo>("init_window", {
          windowId: label,
        });
        setTabs(state.tabs);
        setActiveTabIndex(state.active_tab_index);
      } catch (error) {
        console.error("Failed to initialize window:", error);
      }
    }

    init();
  }, []);

  useEffect(() => {
    // Listen for menu events
    const handleMenuCloseTab = () => {
      if (tabs.length > 0 && activeTabIndex < tabs.length) {
        const activeTab = tabs[activeTabIndex];
        closeTab(activeTab.tab_id);
      }
    };

    const handleMenuDuplicateTab = () => {
      if (tabs.length > 0 && activeTabIndex < tabs.length) {
        const activeTab = tabs[activeTabIndex];
        duplicateTab(activeTab.tab_id);
      }
    };

    window.addEventListener("menu-close-tab", handleMenuCloseTab);
    window.addEventListener("menu-duplicate-tab", handleMenuDuplicateTab);

    return () => {
      window.removeEventListener("menu-close-tab", handleMenuCloseTab);
      window.removeEventListener("menu-duplicate-tab", handleMenuDuplicateTab);
    };
  }, [tabs, activeTabIndex]);

  const refreshTabs = async () => {
    if (!windowId) return;

    try {
      const state = await invoke<WindowStateInfo>("get_window_state", {
        windowId,
      });
      setTabs(state.tabs);
      setActiveTabIndex(state.active_tab_index);
    } catch (error) {
      console.error("Failed to refresh tabs:", error);
    }
  };

  const createTab = async () => {
    if (!windowId) return;

    try {
      await invoke("create_tab", { windowId });
      await refreshTabs();
    } catch (error) {
      console.error("Failed to create tab:", error);
      alert(`Failed to create tab: ${error}`);
    }
  };

  const closeTab = async (tabId: string) => {
    if (!windowId) return;

    try {
      await invoke("close_tab", { windowId, tabId });
      await refreshTabs();
    } catch (error) {
      console.error("Failed to close tab:", error);
      alert(`Failed to close tab: ${error}`);
    }
  };

  const switchTab = async (index: number) => {
    if (!windowId) return;

    try {
      await invoke("switch_tab", { windowId, tabIndex: index });
      setActiveTabIndex(index);
    } catch (error) {
      console.error("Failed to switch tab:", error);
    }
  };

  const duplicateTab = async (tabId: string) => {
    if (!windowId) return;

    try {
      await invoke("duplicate_tab", { windowId, tabId });
      await refreshTabs();
    } catch (error) {
      console.error("Failed to duplicate tab:", error);
      alert(`Failed to duplicate tab: ${error}`);
    }
  };

  const value: WindowContextValue = {
    windowId,
    tabs,
    activeTabIndex,
    createTab,
    closeTab,
    switchTab,
    duplicateTab,
    refreshTabs,
  };

  return (
    <WindowContext.Provider value={value}>
      {children}
    </WindowContext.Provider>
  );
}

export function useWindow(): WindowContextValue {
  const context = useContext(WindowContext);
  if (!context) {
    throw new Error("useWindow must be used within WindowProvider");
  }
  return context;
}
