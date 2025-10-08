import { createContext } from "preact";
import { useContext } from "preact/hooks";

interface TabContextValue {
  tabId: string;
}

const TabContext = createContext<TabContextValue | null>(null);

export function TabProvider(
  { tabId, children }: { tabId: string; children: preact.ComponentChildren },
) {
  const value: TabContextValue = {
    tabId,
  };

  return (
    <TabContext.Provider value={value}>
      {children}
    </TabContext.Provider>
  );
}

export function useTab(): TabContextValue {
  const context = useContext(TabContext);
  if (!context) {
    throw new Error("useTab must be used within TabProvider");
  }
  return context;
}
