import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands, type TtsStatus } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { ShortcutInput } from "../ShortcutInput";
import { VoicesCard } from "./VoicesCard";
import { TranslateCard } from "./TranslateCard";
import { VoiceChangerCard } from "./VoiceChangerCard";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { Input } from "../../ui/Input";
import { Textarea } from "../../ui/Textarea";
import { Button } from "../../ui/Button";
import Badge from "../../ui/Badge";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Slider } from "../../ui/Slider";
import { Select } from "../../ui/Select";
import { ReadingCard } from "./ReadingCard";
import { SummaryCard } from "./SummaryCard";
import { usePersistentState } from "../../../hooks/usePersistentState";

const badgeVariant = (
  phase: TtsStatus["phase"] | undefined,
): "primary" | "success" | "secondary" => {
  switch (phase) {
    case "ready":
    case "speaking":
      return "success";
    case "starting":
      return "primary";
    default:
      return "secondary";
  }
};

export const TtsSettings = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [status, setStatus] = useState<TtsStatus | null>(null);
  // The text you were about to have read out survives leaving the page —
  // losing a pasted article because you glanced at the model list is the
  // kind of loss nobody forgives.
  const [text, setText] = usePersistentState<string>("tts.text", "");
  const [startingSeconds, setStartingSeconds] = useState(0);
  const [lastError, setLastError] = useState<string | null>(null);
  const [speakProgress, setSpeakProgress] = useState<{
    position: number;
    total: number;
  } | null>(null);
  const [currentSentence, setCurrentSentence] = useState<string | null>(null);
  const startingTimer = useRef<number | null>(null);

  useEffect(() => {
    commands.ttsServerStatus().then((r) => {
      if (r.status === "ok") setStatus(r.data);
    });
    const un = listen<TtsStatus>("tts-state-changed", (e) => {
      setStatus(e.payload);
      if (e.payload.phase === "error" && e.payload.message) {
        setLastError(e.payload.message);
      } else if (
        e.payload.phase === "ready" ||
        e.payload.phase === "speaking"
      ) {
        setLastError(null);
      }
    });
    const unProgress = listen<{ position: number; total: number }>(
      "tts-speak-progress",
      (e) => setSpeakProgress(e.payload),
    );
    const unSentence = listen<{ context: string; text: string }>(
      "tts-current-sentence",
      (e) => {
        if (e.payload.context === "speak") setCurrentSentence(e.payload.text);
      },
    );
    return () => {
      un.then((f) => f());
      unProgress.then((f) => f());
      unSentence.then((f) => f());
    };
  }, []);

  // Sekundenzähler nur während des Serverstarts.
  useEffect(() => {
    if (status?.phase === "starting") {
      if (startingTimer.current === null) {
        setStartingSeconds(0);
        startingTimer.current = window.setInterval(
          () => setStartingSeconds((s) => s + 1),
          1000,
        );
      }
    } else if (startingTimer.current !== null) {
      window.clearInterval(startingTimer.current);
      startingTimer.current = null;
      setStartingSeconds(0);
    }
    return () => {
      if (startingTimer.current !== null) {
        window.clearInterval(startingTimer.current);
        startingTimer.current = null;
      }
    };
  }, [status?.phase]);

  const phase = status?.phase ?? "stopped";
  const speaking = phase === "speaking";
  const starting = phase === "starting";

  const speak = async () => {
    setLastError(null);
    setSpeakProgress(null);
    const result = await commands.ttsSpeakText(text);
    if (result.status === "error") setLastError(result.error);
  };

  const pauseSpeaking = () => {
    void commands.ttsCancel();
  };

  const resumeSpeaking = async () => {
    setLastError(null);
    const result = await commands.ttsSpeakResume();
    if (result.status === "error") setLastError(result.error);
  };

  const canResume =
    !speaking &&
    speakProgress !== null &&
    speakProgress.position < speakProgress.total;

  const startServer = async () => {
    setLastError(null);
    const result = await commands.ttsServerStart();
    if (result.status === "error") setLastError(result.error);
  };

  const stopServer = () => {
    void commands.ttsServerStop();
  };

  const showVramHint =
    starting && (startingSeconds >= 120 || status?.message === "vram");

  return (
    <div className="w-full space-y-6">
      <SettingsGroup title={t("tts.title")}>
        <SettingContainer
          title={t("tts.serverTitle")}
          description={t("tts.description")}
          grouped={true}
          layout="horizontal"
        >
          <div className="flex items-center gap-2">
            <Badge variant={badgeVariant(phase)}>
              {t(`tts.status.${phase}`, { seconds: startingSeconds })}
            </Badge>
            {phase === "stopped" || phase === "error" ? (
              <Button size="sm" variant="secondary" onClick={startServer}>
                {t("tts.serverStart")}
              </Button>
            ) : (
              <Button
                size="sm"
                variant="secondary"
                onClick={stopServer}
                disabled={!status?.owns_server}
                title={
                  status?.owns_server ? undefined : t("tts.externalServerHint")
                }
              >
                {t("tts.serverStop")}
              </Button>
            )}
          </div>
        </SettingContainer>
        {showVramHint && (
          <p className="px-4 pb-2 text-sm text-text/70">{t("tts.vramHint")}</p>
        )}
        {lastError && (
          <p className="px-4 pb-2 text-sm text-red-500 break-words">
            {lastError}
          </p>
        )}
        <div className="px-4 pb-4 space-y-2">
          <Textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={t("tts.inputPlaceholder")}
            rows={5}
            className="w-full"
          />
          <div className="flex gap-2 items-center flex-wrap">
            <Button
              onClick={speak}
              disabled={text.trim().length === 0 || starting}
            >
              {t("tts.speak")}
            </Button>
            <Button
              variant="secondary"
              onClick={pauseSpeaking}
              disabled={!speaking}
            >
              {t("tts.pause")}
            </Button>
            {canResume && (
              <Button variant="secondary" onClick={resumeSpeaking}>
                {t("tts.resume")}
              </Button>
            )}
            {speakProgress && (
              <span className="text-xs text-text/60">
                {t("tts.sentenceProgress", {
                  position: speakProgress.position,
                  total: speakProgress.total,
                })}
              </span>
            )}
          </div>
          {speaking && currentSentence && (
            <p className="text-sm italic text-text/70 border-s-2 border-logo-primary ps-2">
              {currentSentence}
            </p>
          )}
        </div>
      </SettingsGroup>

      <ReadingCard />

      <SummaryCard />

      <VoicesCard />

      <TranslateCard />

      <VoiceChangerCard />

      <SettingsGroup title={t("tts.settingsTitle")}>
        <ShortcutInput shortcutId="speak_clipboard" grouped={true} />
        <Slider
          value={getSetting("tts_volume") ?? 1.0}
          onChange={(value) => updateSetting("tts_volume", value)}
          min={0}
          max={1}
          step={0.05}
          formatValue={(value) => `${Math.round(value * 100)}%`}
          label={t("tts.settings.volume")}
          description={t("tts.settings.volumeDescription")}
          grouped={true}
        />
        <Slider
          value={getSetting("tts_speed") ?? 1.0}
          onChange={(value) => updateSetting("tts_speed", value)}
          min={0.5}
          max={2}
          step={0.05}
          formatValue={(value) => `${value.toFixed(2)}×`}
          label={t("tts.settings.speed")}
          description={t("tts.settings.speedDescription")}
          grouped={true}
        />
        <SettingContainer
          title={t("tts.settings.exportFormat")}
          description={t("tts.settings.exportFormatDescription")}
          grouped={true}
          layout="horizontal"
        >
          <div className="w-36">
            {/* Formatnamen sind Eigennamen — bewusst nicht übersetzt. */}
            <Select
              value={getSetting("tts_export_format") ?? "wav"}
              options={[
                { value: "wav", label: "WAV" },
                { value: "mp3", label: "MP3" },
                { value: "opus", label: "Opus" },
              ]}
              isClearable={false}
              onChange={(value) => {
                if (value) updateSetting("tts_export_format", value);
              }}
            />
          </div>
        </SettingContainer>
        <SettingContainer
          title={t("tts.settings.fishDir")}
          description={t("tts.settings.fishDirDescription")}
          grouped={true}
          layout="stacked"
        >
          <Input
            type="text"
            value={getSetting("tts_fish_dir") ?? ""}
            onChange={(e) => updateSetting("tts_fish_dir", e.target.value)}
            disabled={isUpdating("tts_fish_dir")}
            className="w-full"
          />
        </SettingContainer>
        <SettingContainer
          title={t("tts.settings.port")}
          description={t("tts.settings.portDescription")}
          grouped={true}
          layout="horizontal"
        >
          <Input
            type="number"
            min="1"
            max="65535"
            value={getSetting("tts_port") ?? 8080}
            onChange={(e) => {
              const value = parseInt(e.target.value, 10);
              if (!isNaN(value) && value > 0 && value <= 65535) {
                updateSetting("tts_port", value);
              }
            }}
            disabled={isUpdating("tts_port")}
            className="w-24"
          />
        </SettingContainer>
        <SettingContainer
          title={t("tts.settings.seed")}
          description={t("tts.settings.seedDescription")}
          grouped={true}
          layout="horizontal"
        >
          <Input
            type="number"
            value={getSetting("tts_seed") ?? 42}
            onChange={(e) => {
              const value = parseInt(e.target.value, 10);
              if (!isNaN(value)) updateSetting("tts_seed", value);
            }}
            disabled={isUpdating("tts_seed")}
            className="w-24"
          />
        </SettingContainer>
        <SettingContainer
          title={t("tts.settings.idleMinutes")}
          description={t("tts.settings.idleMinutesDescription")}
          grouped={true}
          layout="horizontal"
        >
          <Input
            type="number"
            min="0"
            max="1440"
            value={getSetting("tts_idle_minutes") ?? 15}
            onChange={(e) => {
              const value = parseInt(e.target.value, 10);
              if (!isNaN(value) && value >= 0) {
                updateSetting("tts_idle_minutes", value);
              }
            }}
            disabled={isUpdating("tts_idle_minutes")}
            className="w-24"
          />
        </SettingContainer>
        <ToggleSwitch
          checked={getSetting("tts_compile") ?? true}
          onChange={(checked) => updateSetting("tts_compile", checked)}
          isUpdating={isUpdating("tts_compile")}
          label={t("tts.settings.compile")}
          description={t("tts.settings.compileDescription")}
          grouped={true}
        />
        <ToggleSwitch
          checked={getSetting("tts_context_menu") ?? false}
          onChange={(checked) => updateSetting("tts_context_menu", checked)}
          isUpdating={isUpdating("tts_context_menu")}
          label={t("tts.settings.contextMenu")}
          description={t("tts.settings.contextMenuDescription")}
          grouped={true}
        />
        <SettingContainer
          title={t("tts.settings.maxChars")}
          description={t("tts.settings.maxCharsDescription")}
          grouped={true}
          layout="horizontal"
        >
          <Input
            type="number"
            min="100"
            max="100000"
            value={getSetting("tts_max_chars") ?? 5000}
            onChange={(e) => {
              const value = parseInt(e.target.value, 10);
              if (!isNaN(value) && value >= 100) {
                updateSetting("tts_max_chars", value);
              }
            }}
            disabled={isUpdating("tts_max_chars")}
            className="w-24"
          />
        </SettingContainer>
      </SettingsGroup>
    </div>
  );
};
