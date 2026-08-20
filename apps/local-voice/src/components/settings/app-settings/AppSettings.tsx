import React from "react";
import { useTranslation } from "react-i18next";
import { GeneralSettings } from "../general/GeneralSettings";
import { AdvancedSettings } from "../advanced/AdvancedSettings";
import { DictationTest } from "../dictation-test/DictationTest";
import { AboutSettings } from "../about/AboutSettings";
import { usePersistentState } from "../../../hooks/usePersistentState";

/**
 * "Einstellungen" — everything that configures the app rather than being a
 * place you work in: the former General and Advanced pages plus the dictation
 * test and the about page, which each used to occupy a sidebar row of their
 * own. The sidebar is now the list of *activities* (history, meetings, models,
 * read aloud); configuration is one entry with tabs.
 *
 * The tab strip scrolls horizontally instead of wrapping — on a narrow window
 * a wrapped strip pushes the content down by a whole row for no gain.
 */
const TABS = [
  { id: "general", labelKey: "sidebar.general", Component: GeneralSettings },
  { id: "app", labelKey: "settings.app.tabs.app", Component: AdvancedSettings },
  {
    id: "dictationTest",
    labelKey: "sidebar.dictationTest",
    Component: DictationTest,
  },
  { id: "about", labelKey: "sidebar.about", Component: AboutSettings },
] as const;

type TabId = (typeof TABS)[number]["id"];

export const AppSettings: React.FC = () => {
  const { t } = useTranslation();
  const [tab, setTab] = usePersistentState<TabId>("settings.tab", "general");
  const active = TABS.find((entry) => entry.id === tab) ?? TABS[0];
  const ActiveComponent = active.Component;

  return (
    <div className="w-full space-y-4">
      <div className="flex gap-1 border-b border-mid-gray/20 overflow-x-auto">
        {TABS.map((entry) => (
          <button
            key={entry.id}
            type="button"
            onClick={() => setTab(entry.id)}
            className={`px-3 py-1.5 text-sm font-medium border-b-2 cursor-pointer whitespace-nowrap ${
              entry.id === active.id
                ? "border-logo-primary text-text"
                : "border-transparent text-text/60 hover:text-text"
            }`}
          >
            {t(entry.labelKey)}
          </button>
        ))}
      </div>
      <ActiveComponent />
    </div>
  );
};
