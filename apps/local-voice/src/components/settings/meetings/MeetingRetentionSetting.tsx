import React from "react";
import { useTranslation } from "react-i18next";
import type { MeetingAudioRetention } from "@/bindings";
import { Dropdown } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";
import { useSettings } from "../../../hooks/useSettings";

const encode = (retention: MeetingAudioRetention | undefined): string => {
  if (!retention) return "after_minutes";
  if (typeof retention === "string") return retention;
  return `days:${retention.days}`;
};

const decode = (value: string): MeetingAudioRetention => {
  if (value === "forever") return "forever";
  if (value.startsWith("days:")) {
    return { days: Number(value.slice("days:".length)) };
  }
  return "after_minutes";
};

/**
 * Closes the loop on Task 12's `meeting_audio_retention` setting — three
 * presets (after minutes / a fixed day count / forever) mapped onto the
 * backend's `MeetingAudioRetention` union.
 */
export const MeetingRetentionSetting: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const value = encode(getSetting("meeting_audio_retention"));

  const options = [
    { value: "after_minutes", label: t("meetings.retention.afterMinutes") },
    { value: "days:3", label: t("meetings.retention.days3") },
    { value: "days:14", label: t("meetings.retention.days14") },
    { value: "days:90", label: t("meetings.retention.days90") },
    { value: "forever", label: t("meetings.retention.forever") },
  ];

  return (
    <SettingContainer
      title={t("meetings.retention.title")}
      description={t("meetings.retention.description")}
      grouped={true}
    >
      <Dropdown
        options={options}
        selectedValue={value}
        onSelect={(v) => updateSetting("meeting_audio_retention", decode(v))}
        placeholder={t("meetings.retention.placeholder")}
        disabled={isUpdating("meeting_audio_retention")}
      />
    </SettingContainer>
  );
};
