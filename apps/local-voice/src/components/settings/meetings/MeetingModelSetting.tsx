import React, { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";
import { useSettings } from "../../../hooks/useSettings";
import { useModelStore } from "../../../stores/modelStore";

/**
 * Dedicated transcription model for meetings/imports. Empty = use the
 * dictation model. Rationale: streaming models are tuned for live dictation;
 * meetings transcribe in batches and profit from batch models. The backend
 * swaps to this model for meeting work and restores the dictation model
 * afterwards (TranscriptionManager::meeting_model_target).
 */
export const MeetingModelSetting: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const { models, loadModels } = useModelStore();

  useEffect(() => {
    if (models.length === 0) void loadModels();
  }, [models.length, loadModels]);

  const value = getSetting("meeting_model") ?? "";

  const options = [
    { value: "", label: t("meetings.model.likeDictation") },
    ...models
      .filter((m) => m.is_downloaded)
      .map((m) => ({ value: m.id, label: m.name })),
  ];

  return (
    <SettingContainer
      title={t("meetings.model.title")}
      description={t("meetings.model.description")}
      grouped={true}
    >
      <Dropdown
        options={options}
        selectedValue={value}
        onSelect={(v) => updateSetting("meeting_model", v === "" ? null : v)}
        placeholder={t("meetings.model.likeDictation")}
        disabled={isUpdating("meeting_model")}
      />
    </SettingContainer>
  );
};
