import { useWindow, WindowProvider } from "./contexts/WindowContext.tsx";
import { TabProvider } from "./contexts/TabContext.tsx";
import { TabBar } from "./components/TabBar.tsx";
import { TabContent } from "./components/TabContent.tsx";

function WindowManager() {
  const { tabs, activeTabIndex, switchTab } = useWindow();

  if (tabs.length === 0) {
    return (
      <div class="app">
        <div class="loading">Initializing...</div>
      </div>
    );
  }

  return (
    <div class="app">
      <TabBar tabs={tabs} activeIndex={activeTabIndex} onSwitch={switchTab} />
      {tabs.map((tab, index) => (
        <TabProvider key={tab.tab_id} tabId={tab.tab_id}>
          <TabContent
            key={tab.tab_id}
            isActive={index === activeTabIndex}
          />
        </TabProvider>
      ))}
    </div>
  );
}

function App() {
  return (
    <WindowProvider>
      <WindowManager />
    </WindowProvider>
  );
}

export default App;
