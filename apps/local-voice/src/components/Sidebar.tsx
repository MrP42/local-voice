import React from "react";
import { useTranslation } from "react-i18next";
import {
  Cog,
  FlaskConical,
  History,
  Sparkles,
  Cpu,
  Users,
  Volume2,
} from "lucide-react";
import LocalVoiceAiLogo from "./icons/LocalVoiceAiLogo";
import { useSettings } from "../hooks/useSettings";
import {
  AppSettings,
  HistorySettings,
  DebugSettings,
  PostProcessingSettings,
  ModelsSettings,
  TtsSettings,
  MeetingsSettings,
} from "./settings";

export type SidebarSection = keyof typeof SECTIONS_CONFIG;

interface IconProps {
  width?: number | string;
  height?: number | string;
  size?: number | string;
  className?: string;
  [key: string]: any;
}

interface SectionConfig {
  labelKey: string;
  icon: React.ComponentType<IconProps>;
  component: React.ComponentType;
  enabled: (settings: any) => boolean;
}

// The sidebar lists what you DO with the app; everything that merely
// configures it lives under one "Einstellungen" entry with tabs (AppSettings).
// General, Advanced, the dictation test and the about page were four separate
// rows before — four rows of navigation for one activity.
export const SECTIONS_CONFIG = {
  history: {
    labelKey: "sidebar.history",
    icon: History,
    component: HistorySettings,
    enabled: () => true,
  },
  meetings: {
    labelKey: "sidebar.meetings",
    icon: Users,
    component: MeetingsSettings,
    enabled: () => true,
  },
  models: {
    labelKey: "sidebar.models",
    icon: Cpu,
    component: ModelsSettings,
    enabled: () => true,
  },
  tts: {
    labelKey: "sidebar.tts",
    icon: Volume2,
    component: TtsSettings,
    enabled: () => true,
  },
  settings: {
    labelKey: "sidebar.settings",
    icon: Cog,
    component: AppSettings,
    enabled: () => true,
  },
  postprocessing: {
    labelKey: "sidebar.postProcessing",
    icon: Sparkles,
    component: PostProcessingSettings,
    enabled: (settings) => settings?.post_process_enabled ?? false,
  },
  debug: {
    labelKey: "sidebar.debug",
    icon: FlaskConical,
    component: DebugSettings,
    enabled: (settings) => settings?.debug_mode ?? false,
  },
} as const satisfies Record<string, SectionConfig>;

export const isSidebarSection = (value: string): value is SidebarSection =>
  Object.prototype.hasOwnProperty.call(SECTIONS_CONFIG, value);

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();

  const availableSections = Object.entries(SECTIONS_CONFIG)
    .filter(([_, config]) => config.enabled(settings))
    .map(([id, config]) => ({ id: id as SidebarSection, ...config }));

  return (
    // Two widths, one breakpoint: below `sm` the rail is icons only (a label
    // column would eat half a narrow window), from `sm` up it is wide enough
    // that the longest label — "Besprechungen" — fits without truncation.
    // That is a change of the interaction model, not cosmetics, which is why
    // it earns a media query (design system, references/responsive.md).
    <nav
      className="flex flex-col w-14 sm:w-52 h-full shrink-0 border-e border-mid-gray/20 px-2 overflow-y-auto"
      aria-label={t("sidebar.ariaLabel")}
    >
      {/* Word mark only — the pictorial mark repeats what the taskbar icon
          already says and cost the labels their width. */}
      <div className="hidden sm:block px-1 py-4">
        <LocalVoiceAiLogo height={22} showMark={false} />
      </div>
      <div className="sm:hidden h-4" />
      <div className="flex flex-col w-full items-center gap-1 pt-2 border-t border-mid-gray/20">
        {availableSections.map((section) => {
          const Icon = section.icon;
          const isActive = activeSection === section.id;
          const label = t(section.labelKey);

          return (
            <button
              key={section.id}
              type="button"
              aria-current={isActive ? "page" : undefined}
              className={`flex gap-2 items-center p-2 w-full rounded-lg cursor-pointer transition-colors justify-center sm:justify-start ${
                isActive
                  ? // Ink auf Gelb (Design-System) — sonst stünde im Dark-Theme
                    // weiße Schrift auf dem gelben Aktiv-Balken.
                    "bg-logo-primary/80 text-on-accent"
                  : "hover:bg-mid-gray/20 hover:opacity-100 opacity-85"
              }`}
              onClick={() => onSectionChange(section.id)}
              title={label}
            >
              <Icon width={22} height={22} className="shrink-0" />
              <span className="hidden sm:block text-sm font-medium text-start min-w-0">
                {label}
              </span>
            </button>
          );
        })}
      </div>
    </nav>
  );
};
