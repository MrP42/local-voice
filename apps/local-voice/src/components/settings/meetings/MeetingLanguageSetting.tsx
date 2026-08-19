import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";
import { useSettings } from "../../../hooks/useSettings";
import { SELECTABLE_LANGUAGES } from "../../../lib/constants/languages";

/**
 * Meetings carry their OWN transcription language intent
 * (`meeting_language`, default "auto") — deliberately separate from the
 * dictation `selected_language`, and meetings never translate: the dictation
 * `translate_to_english` setting must not leak into meeting transcripts
 * (M8 acceptance ruling). A per-meeting translation option, if ever wanted,
 * would be an explicit UI feature, not a settings pass-through.
 */
export const MeetingLanguageSetting: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const value = getSetting("meeting_language") || "auto";

  const options = SELECTABLE_LANGUAGES.map((lang) =>
    lang.value === "auto"
      ? { value: "auto", label: t("meetings.language.auto") }
      : { value: lang.value, label: lang.label },
  );

  return (
    <SettingContainer
      title={t("meetings.language.title")}
      description={t("meetings.language.description")}
      grouped={true}
    >
      <Dropdown
        options={options}
        selectedValue={value}
        onSelect={(v) => updateSetting("meeting_language", v)}
        placeholder={t("meetings.language.auto")}
        disabled={isUpdating("meeting_language")}
      />
    </SettingContainer>
  );
};
