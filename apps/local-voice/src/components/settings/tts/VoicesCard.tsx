import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { commands } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Input } from "../../ui/Input";
import { Textarea } from "../../ui/Textarea";
import { Button } from "../../ui/Button";
import Badge from "../../ui/Badge";

type Mode =
  | { kind: "idle" }
  | { kind: "recording" }
  | { kind: "review"; source: "recording" | { wavPath: string } };

export const VoicesCard = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const [voices, setVoices] = useState<string[]>([]);
  const [mode, setMode] = useState<Mode>({ kind: "idle" });
  const [name, setName] = useState("");
  const [transcript, setTranscript] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [recordSeconds, setRecordSeconds] = useState(0);
  const recordTimer = useRef<number | null>(null);

  const activeVoice = getSetting("tts_voice") ?? null;

  const refreshVoices = useCallback(async () => {
    const result = await commands.ttsListVoices();
    if (result.status === "ok") setVoices(result.data);
  }, []);

  useEffect(() => {
    void refreshVoices();
  }, [refreshVoices]);

  useEffect(() => {
    if (mode.kind === "recording") {
      setRecordSeconds(0);
      recordTimer.current = window.setInterval(
        () => setRecordSeconds((s) => s + 1),
        1000,
      );
    } else if (recordTimer.current !== null) {
      window.clearInterval(recordTimer.current);
      recordTimer.current = null;
    }
    return () => {
      if (recordTimer.current !== null) {
        window.clearInterval(recordTimer.current);
        recordTimer.current = null;
      }
    };
  }, [mode.kind]);

  const startRecording = async () => {
    setError(null);
    const result = await commands.ttsRecordReferenceStart();
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setMode({ kind: "recording" });
  };

  const stopRecording = async () => {
    setBusy(true);
    const result = await commands.ttsRecordReferenceStop();
    setBusy(false);
    if (result.status === "error") {
      setError(result.error);
      setMode({ kind: "idle" });
      return;
    }
    setTranscript(result.data);
    setMode({ kind: "review", source: "recording" });
  };

  const pickFile = async () => {
    setError(null);
    const picked = await open({
      multiple: false,
      filters: [{ name: "WAV", extensions: ["wav"] }],
    });
    if (typeof picked !== "string") return;
    setTranscript("");
    setMode({ kind: "review", source: { wavPath: picked } });
  };

  const save = async () => {
    if (mode.kind !== "review") return;
    setBusy(true);
    setError(null);
    const result =
      mode.source === "recording"
        ? await commands.ttsSaveVoice(name, transcript)
        : await commands.ttsImportVoice(
            name,
            mode.source.wavPath,
            transcript.trim().length > 0 ? transcript : null,
          );
    setBusy(false);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    const id = typeof result.data === "string" ? result.data : result.data.id;
    setMode({ kind: "idle" });
    setName("");
    setTranscript("");
    await refreshVoices();
    // Neue Stimme direkt aktiv schalten — das ist praktisch immer die Absicht.
    await updateSetting("tts_voice", id);
  };

  const discard = () => {
    setMode({ kind: "idle" });
    setName("");
    setTranscript("");
    setError(null);
  };

  const remove = async (id: string) => {
    setError(null);
    const result = await commands.ttsDeleteVoice(id);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    if (activeVoice === id) {
      await updateSetting("tts_voice", null);
    }
    await refreshVoices();
  };

  return (
    <SettingsGroup title={t("tts.voices.title")}>
      <div className="px-4 py-3 space-y-3">
        <p className="text-sm text-text/70">{t("tts.voices.description")}</p>
        {error && <p className="text-sm text-red-500 break-words">{error}</p>}

        <div className="space-y-1">
          <div className="flex items-center justify-between gap-2 py-1">
            <span className="text-sm">{t("tts.voices.defaultVoice")}</span>
            <div className="flex items-center gap-2">
              {activeVoice === null ? (
                <Badge variant="success">{t("tts.voices.active")}</Badge>
              ) : (
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => updateSetting("tts_voice", null)}
                >
                  {t("tts.voices.activate")}
                </Button>
              )}
            </div>
          </div>
          {voices.map((id) => (
            <div
              key={id}
              className="flex items-center justify-between gap-2 py-1"
            >
              <span className="text-sm font-medium">{id}</span>
              <div className="flex items-center gap-2">
                {activeVoice === id ? (
                  <Badge variant="success">{t("tts.voices.active")}</Badge>
                ) : (
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => updateSetting("tts_voice", id)}
                  >
                    {t("tts.voices.activate")}
                  </Button>
                )}
                <Button
                  size="sm"
                  variant="danger-ghost"
                  onClick={() => remove(id)}
                >
                  {t("tts.voices.delete")}
                </Button>
              </div>
            </div>
          ))}
          {voices.length === 0 && (
            <p className="text-sm text-text/60">{t("tts.voices.empty")}</p>
          )}
        </div>

        {mode.kind === "idle" && (
          <div className="flex gap-2">
            <Button onClick={startRecording}>{t("tts.voices.record")}</Button>
            <Button variant="secondary" onClick={pickFile}>
              {t("tts.voices.import")}
            </Button>
          </div>
        )}

        {mode.kind === "recording" && (
          <div className="flex items-center gap-3">
            <Badge variant="primary">
              {t("tts.voices.recording", { seconds: recordSeconds })}
            </Badge>
            <Button onClick={stopRecording} disabled={busy}>
              {t("tts.voices.stopRecording")}
            </Button>
            <span className="text-sm text-text/60">
              {t("tts.voices.recordingHint")}
            </span>
          </div>
        )}

        {mode.kind === "review" && (
          <div className="space-y-2">
            <Input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("tts.voices.namePlaceholder")}
              className="w-64"
            />
            <Textarea
              value={transcript}
              onChange={(e) => setTranscript(e.target.value)}
              placeholder={
                mode.source === "recording"
                  ? t("tts.voices.transcriptPlaceholder")
                  : t("tts.voices.transcriptImportPlaceholder")
              }
              rows={4}
            />
            <div className="flex gap-2">
              <Button
                onClick={save}
                disabled={busy || name.trim().length === 0}
              >
                {t("tts.voices.save")}
              </Button>
              <Button variant="ghost" onClick={discard} disabled={busy}>
                {t("tts.voices.discard")}
              </Button>
            </div>
          </div>
        )}
      </div>
    </SettingsGroup>
  );
};
