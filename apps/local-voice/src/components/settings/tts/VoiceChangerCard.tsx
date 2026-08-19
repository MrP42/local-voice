import { useState } from "react";
import { useTranslation } from "react-i18next";
import { open, save } from "@tauri-apps/plugin-dialog";
import { commands } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import Badge from "../../ui/Badge";

export const VoiceChangerCard = () => {
  const { t } = useTranslation();
  const [transcript, setTranscript] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [recording, setRecording] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);

  const startRecording = async () => {
    setError(null);
    setTranscript(null);
    setSaved(null);
    const result = await commands.ttsVoicechangeRecordStart();
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setRecording(true);
  };

  const stopRecording = async () => {
    setRecording(false);
    setBusy(true);
    const result = await commands.ttsVoicechangeRecordStop();
    setBusy(false);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setTranscript(result.data);
  };

  const pickFile = async () => {
    setError(null);
    setSaved(null);
    const picked = await open({
      multiple: false,
      filters: [
        {
          name: "Audio/Video",
          extensions: [
            "wav", "mp3", "m4a", "aac", "flac", "ogg", "opus",
            "wma", "mp4", "mov", "mkv", "webm", "avi",
          ],
        },
      ],
    });
    if (typeof picked !== "string") return;
    setBusy(true);
    const result = await commands.ttsVoicechangeFile(picked);
    setBusy(false);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setTranscript(result.data);
  };

  const exportWav = async () => {
    if (!transcript) return;
    const formatResult = await commands.ttsExportFormat();
    const format = formatResult.status === "ok" ? formatResult.data : "wav";
    const target = await save({
      filters: [{ name: format.toUpperCase(), extensions: [format] }],
      defaultPath: `stimmwechsler.${format}`,
    });
    if (typeof target !== "string") return;
    setBusy(true);
    setError(null);
    const result = await commands.ttsSynthesizeToFile(transcript, target);
    setBusy(false);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setSaved(target);
  };

  return (
    <SettingsGroup title={t("tts.voiceChanger.title")}>
      <div className="px-4 py-3 space-y-3">
        <p className="text-sm text-text/70">
          {t("tts.voiceChanger.description")}
        </p>
        {error && <p className="text-sm text-red-500 break-words">{error}</p>}

        <div className="flex gap-2 items-center">
          {recording ? (
            <Button variant="danger" onClick={stopRecording} disabled={busy}>
              {t("tts.voiceChanger.stopRecording")}
            </Button>
          ) : (
            <Button onClick={startRecording} disabled={busy}>
              {t("tts.voiceChanger.record")}
            </Button>
          )}
          <Button variant="secondary" onClick={pickFile} disabled={busy || recording}>
            {t("tts.voiceChanger.pickFile")}
          </Button>
          {recording && (
            <Badge variant="primary">{t("tts.voiceChanger.recording")}</Badge>
          )}
          {busy && (
            <Badge variant="secondary">{t("tts.voiceChanger.working")}</Badge>
          )}
        </div>

        {transcript && (
          <div className="space-y-2">
            <div className="text-sm">
              <span className="font-medium">{t("tts.voiceChanger.heard")}</span>{" "}
              <span className="text-text/80">{transcript}</span>
            </div>
            <div className="flex gap-2 items-center">
              <Button size="sm" variant="secondary" onClick={exportWav} disabled={busy}>
                {t("tts.voiceChanger.export")}
              </Button>
              {saved && (
                <span className="text-xs text-text/60 break-all">
                  {t("tts.voiceChanger.savedTo", { path: saved })}
                </span>
              )}
            </div>
          </div>
        )}
      </div>
    </SettingsGroup>
  );
};
