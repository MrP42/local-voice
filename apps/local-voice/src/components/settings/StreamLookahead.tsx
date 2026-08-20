import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";

/**
 * Cache-aware streaming look-ahead (`att_context_right`) — how many encoder
 * frames the model may wait for before it commits a word.
 *
 * This is the quality/latency dial of the streaming Parakeet family (Nemotron
 * 3.5 ASR among them): more look-ahead means the model has heard more of what
 * comes next before deciding, so it revises less and reads better; less means
 * text appears sooner.
 *
 * The steps are NOT free-form. The model is trained on a fixed menu and the
 * engine refuses to start a stream on any other value — measured:
 * `att_context_right=2 not in model's training menu; available: 13 6 3 0`,
 * which cost the whole dictation. `SUPPORTED_LOOKAHEAD_FRAMES` in settings.rs
 * is that menu; this list must not grow past it.
 *
 * A frame is ~80 ms of audio (FastConformer, 8x subsampling of 10 ms hops), so
 * the labels state the resulting wait. 0 is deliberately absent as a choice of
 * its own: the backend treats a non-positive value as "let the model decide",
 * and zero look-ahead damages the text badly enough that nobody should land on
 * it by accident.
 */
const STEPS = [
  { frames: 3, labelKey: "settings.models.lookahead.fast" },
  { frames: 6, labelKey: "settings.models.lookahead.balanced" },
  { frames: 13, labelKey: "settings.models.lookahead.accurate" },
] as const;

export const StreamLookahead: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();

  const current = getSetting("stream_lookahead_frames") ?? 6;
  const known = STEPS.some((step) => step.frames === current);

  const options = [
    { value: "0", label: t("settings.models.lookahead.modelDefault") },
    ...STEPS.map((step) => ({
      value: String(step.frames),
      label: t(step.labelKey),
    })),
  ];

  return (
    <SettingContainer
      title={t("settings.models.lookahead.title")}
      description={t("settings.models.lookahead.description")}
      descriptionMode="tooltip"
      grouped={true}
    >
      <Dropdown
        options={options}
        // A value written straight into the settings file that isn't on the
        // menu falls back to "model default" here, which is exactly what the
        // backend does with it.
        selectedValue={known ? String(current) : "0"}
        onSelect={(value) =>
          updateSetting("stream_lookahead_frames", Number(value))
        }
        disabled={isUpdating("stream_lookahead_frames")}
      />
    </SettingContainer>
  );
};
