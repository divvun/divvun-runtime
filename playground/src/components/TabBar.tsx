import type { TabInfo } from "../types";
import { useWindow } from "../contexts/WindowContext";

interface TabBarProps {
  tabs: TabInfo[];
  activeIndex: number;
  onSwitch: (index: number) => void;
}

export function TabBar({ tabs, activeIndex, onSwitch }: TabBarProps) {
  const { createTab, closeTab } = useWindow();

  const handleCloseTab = (e: Event, tabId: string) => {
    e.stopPropagation();
    closeTab(tabId);
  };

  const getTabTitle = (tab: TabInfo) => {
    return tab.bundle_name || "New Tab";
  };

  return (
    <div class="tab-bar">
      <div class="tab-list">
        {tabs.map((tab, index) => (
          <div
            key={tab.tab_id}
            class={`tab ${index === activeIndex ? "active" : ""}`}
            onClick={() => onSwitch(index)}
          >
            <span class="tab-title">{getTabTitle(tab)}</span>
            {tabs.length > 1 && (
              <button
                type="button"
                class="tab-close"
                onClick={(e) => handleCloseTab(e, tab.tab_id)}
                title="Close tab"
              >
                ×
              </button>
            )}
          </div>
        ))}
      </div>
      <button
        type="button"
        class="new-tab-button"
        onClick={createTab}
        title="New tab (Cmd+T)"
      >
        +
      </button>
    </div>
  );
}
