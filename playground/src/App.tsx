import "./App.css";
import { useWindow, WindowProvider } from "./contexts/WindowContext";
import { TabProvider } from "./contexts/TabContext";
import { TabBar } from "./components/TabBar";
import { TabContent } from "./components/TabContent";

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
