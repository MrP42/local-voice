import React from "react";
import { useTranslation } from "react-i18next";
import { DictationTab } from "./DictationTab";
import { SoundTab } from "./SoundTab";
import { AppTab } from "./AppTab";
import { PostProcessingSettings } from "../post-processing/PostProcessingSettings";
import { DictationTest } from "../dictation-test/DictationTest";
import { AboutSettings } from "../about/AboutSettings";
import { DebugSettings } from "../debug/DebugSettings";
import { useSettings } from "../../../hooks/useSettings";
import { usePersistentState } from "../../../hooks/usePersistentState";

/**
 * "Einstellungen" — the single place where the app is configured.
 *
 * The sidebar lists what you DO (history, meetings, models, read aloud);
 * everything that merely configures the app is one entry with tabs. The tabs
 * are named after the question you arrive with — dictation, sound,
 * post-processing, the app itself — not after how deep a setting sits in the
 * code. The old split into "General" and "Advanced" told nobody where to look:
 * the microphone was general, the paste method advanced, and both belong to
 * the same act of dictating.
 *
 * Adding a setting means putting it in the group it belongs to, here. It does
 * not mean a new tab, and never a new sidebar entry.
 */
const TABS = [
  {
    id: "dictation",
    labelKey: "settings.app.tabs.dictation",
    Component: DictationTab,
    enabled: () => true,
  },
  {
    id: "sound",
    labelKey: "settings.app.tabs.sound",
    Component: SoundTab,
    enabled: () => true,
  },
  {
    id: "postprocessing",
    labelKey: "sidebar.postProcessing",
    Component: PostProcessingSettings,
    enabled: () => true,
  },
  {
    id: "app",
    labelKey: "settings.app.tabs.app",
    Component: AppTab,
    enabled: () => true,
  },
  {
    id: "dictationTest",
    labelKey: "sidebar.dictationTest",
    Component: DictationTest,
    enabled: () => true,
  },
  {
    id: "about",
    labelKey: "sidebar.about",
    Component: AboutSettings,
    enabled: () => true,
  },
  {
    id: "debug",
    labelKey: "sidebar.debug",
    Component: DebugSettings,
    enabled: (settings: any) => settings?.debug_mode ?? false,
  },
] as const;

type TabId = (typeof TABS)[number]["id"];

const isTabId = (value: string): value is TabId =>
  TABS.some((tab) => tab.id === value);

export const AppSettings: React.FC = () => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const [tab, setTab] = usePersistentState<TabId>(
    "settings.tab",
    "dictation",
    isTabId,
  );

  const available = TABS.filter((entry) => entry.enabled(settings));
  // A stored tab can point at one that is hidden again (debug switched off).
  const active = available.find((entry) => entry.id === tab) ?? available[0];
  const ActiveComponent = active.Component;

  return (
    <div className="w-full space-y-4">
      {/* Scrolls rather than wraps: on a narrow window a wrapped strip pushes
          the content down by a whole row for no gain. */}
      <div
        role="tablist"
        className="flex gap-1 border-b border-mid-gray/20 overflow-x-auto"
      >
        {available.map((entry) => (
          <button
            key={entry.id}
            type="button"
            role="tab"
            aria-selected={entry.id === active.id}
            onClick={() => setTab(entry.id)}
            className={`px-3 py-1.5 text-sm font-medium border-b-2 cursor-pointer whitespace-nowrap transition-colors ${
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
