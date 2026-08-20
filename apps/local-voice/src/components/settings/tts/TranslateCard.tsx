import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Textarea } from "../../ui/Textarea";
import { Button } from "../../ui/Button";
import Badge from "../../ui/Badge";
import { Select } from "../../ui/Select";
import {
  usePersistentNullableText,
  usePersistentState,
} from "../../../hooks/usePersistentState";

/// Zielsprachen als englische Namen (so erwartet sie der Übersetzungs-Prompt);
/// die Labels sind Eigennamen der Sprachen, keine UI-Texte.
const TARGET_LANGS = [
  { value: "German", label: "Deutsch" },
  { value: "English", label: "English" },
  { value: "French", label: "Français" },
  { value: "Spanish", label: "Español" },
  { value: "Italian", label: "Italiano" },
  { value: "Portuguese", label: "Português" },
  { value: "Dutch", label: "Nederlands" },
  { value: "Polish", label: "Polski" },
  { value: "Japanese", label: "日本語" },
  { value: "Chinese", label: "中文" },
];

export const TranslateCard = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const [input, setInput] = usePersistentState<string>(
    "tts.translate.input",
    "",
  );
  const [transcript, setTranscript] = usePersistentNullableText(
    "tts.translate.transcript",
  );
  const [translation, setTranslation] = usePersistentNullableText(
    "tts.translate.translation",
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [recording, setRecording] = useState(false);

  const targetLang = getSetting("tts_translate_lang") ?? "English";

  const translateText = async () => {
    setBusy(true);
    setError(null);
    setTranscript(null);
    const result = await commands.ttsTranslateSpeak(input, targetLang);
    setBusy(false);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setTranslation(result.data);
  };

  const startRecording = async () => {
    setError(null);
    setTranscript(null);
    setTranslation(null);
    const result = await commands.ttsRecordTranslateStart();
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setRecording(true);
  };

  const stopRecording = async () => {
    setRecording(false);
    setBusy(true);
    const result = await commands.ttsRecordTranslateStop(targetLang);
    setBusy(false);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setTranscript(result.data.transcript);
    setTranslation(result.data.translation);
  };

  return (
    <SettingsGroup title={t("tts.translate.title")}>
      <div className="px-4 py-3 space-y-3">
        <p className="text-sm text-text/70">{t("tts.translate.description")}</p>
        {error && <p className="text-sm text-red-500 break-words">{error}</p>}

        <div className="flex items-center gap-2">
          <span className="text-sm">{t("tts.translate.targetLang")}</span>
          <div className="w-44">
            <Select
              value={targetLang}
              options={TARGET_LANGS}
              isClearable={false}
              onChange={(value) => {
                if (value) updateSetting("tts_translate_lang", value);
              }}
            />
          </div>
        </div>

        <Textarea
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder={t("tts.translate.inputPlaceholder")}
          rows={3}
          className="w-full"
        />
        <div className="flex gap-2 items-center">
          <Button
            onClick={translateText}
            disabled={busy || recording || input.trim().length === 0}
          >
            {t("tts.translate.translateSpeak")}
          </Button>
          {recording ? (
            <Button variant="danger" onClick={stopRecording} disabled={busy}>
              {t("tts.translate.stopRecording")}
            </Button>
          ) : (
            <Button
              variant="secondary"
              onClick={startRecording}
              disabled={busy}
            >
              {t("tts.translate.record")}
            </Button>
          )}
          {recording && (
            <Badge variant="primary">{t("tts.translate.recording")}</Badge>
          )}
          {busy && (
            <Badge variant="secondary">{t("tts.translate.working")}</Badge>
          )}
        </div>

        {/* whitespace-pre-wrap, weil HTML Zeilenumbrueche sonst zu Leerzeichen
            faltet: die Uebersetzung kommt mit den Absaetzen des Originals
            zurueck, wurde hier aber als eine einzige Textwand ausgegeben.
            Die Beschriftung steht ueber dem Text statt davor, damit der erste
            Absatz an derselben Kante beginnt wie alle folgenden. */}
        {transcript && (
          <div className="text-sm space-y-1">
            <div className="font-medium">{t("tts.translate.heard")}</div>
            <div className="text-text/80 whitespace-pre-wrap">{transcript}</div>
          </div>
        )}
        {translation && (
          <div className="text-sm space-y-1">
            <div className="font-medium">{t("tts.translate.result")}</div>
            <div className="text-text/80 whitespace-pre-wrap">
              {translation}
            </div>
          </div>
        )}
        <p className="text-xs text-text/50">
          {t("tts.translate.providerHint")}
        </p>
      </div>
    </SettingsGroup>
  );
};
