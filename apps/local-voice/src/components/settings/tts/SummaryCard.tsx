import { useState } from "react";
import { useTranslation } from "react-i18next";
import { open, save } from "@tauri-apps/plugin-dialog";
import { commands } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Textarea } from "../../ui/Textarea";
import { Button } from "../../ui/Button";
import Badge from "../../ui/Badge";

export const SummaryCard = () => {
  const { t } = useTranslation();
  const [source, setSource] = useState("");
  const [sourceName, setSourceName] = useState<string | null>(null);
  const [summary, setSummary] = useState("");
  const [length, setLength] = useState("mittel");
  const [detail, setDetail] = useState("ausgewogen");
  const [audience, setAudience] = useState("allgemein");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);

  const loadDocument = async () => {
    setError(null);
    const picked = await open({
      multiple: false,
      filters: [{ name: "Dokumente", extensions: ["txt", "md", "pdf", "docx"] }],
    });
    if (typeof picked !== "string") return;
    setBusy(true);
    const result = await commands.ttsExtractDocument(picked);
    setBusy(false);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setSource(result.data);
    setSourceName(picked.split(/[\\/]/).pop() ?? picked);
  };

  const summarize = async () => {
    setBusy(true);
    setError(null);
    setSaved(null);
    const result = await commands.ttsSummarizeText(source, {
      length,
      detail,
      audience,
    });
    setBusy(false);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setSummary(result.data);
  };

  const speakSummary = async () => {
    setError(null);
    const result = await commands.ttsSpeakText(summary);
    if (result.status === "error") setError(result.error);
  };

  const exportSummary = async () => {
    setError(null);
    const formatResult = await commands.ttsExportFormat();
    const format = formatResult.status === "ok" ? formatResult.data : "wav";
    const target = await save({
      filters: [{ name: format.toUpperCase(), extensions: [format] }],
      defaultPath: `zusammenfassung.${format}`,
    });
    if (typeof target !== "string") return;
    setBusy(true);
    const result = await commands.ttsSynthesizeToFile(summary, target);
    setBusy(false);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setSaved(target);
  };

  const selectClass =
    "text-sm bg-transparent border border-mid-gray/40 rounded-md px-2 py-1";

  return (
    <SettingsGroup title={t("tts.summary.title")}>
      <div className="px-4 py-3 space-y-3">
        <p className="text-sm text-text/70">{t("tts.summary.description")}</p>
        {error && <p className="text-sm text-red-500 break-words">{error}</p>}

        <div className="flex gap-2 items-center flex-wrap">
          <Button variant="secondary" onClick={loadDocument} disabled={busy}>
            {t("tts.summary.loadDocument")}
          </Button>
          {sourceName && (
            <span className="text-xs text-text/60 truncate">{sourceName}</span>
          )}
        </div>
        <Textarea
          value={source}
          onChange={(e) => {
            setSource(e.target.value);
            setSourceName(null);
          }}
          placeholder={t("tts.summary.sourcePlaceholder")}
          rows={4}
          className="w-full"
        />

        <div className="flex gap-3 items-center flex-wrap">
          <label className="flex items-center gap-1 text-sm">
            {t("tts.summary.length")}
            <select
              className={selectClass}
              value={length}
              onChange={(e) => setLength(e.target.value)}
            >
              <option value="kurz">{t("tts.summary.lengths.short")}</option>
              <option value="mittel">{t("tts.summary.lengths.medium")}</option>
              <option value="lang">{t("tts.summary.lengths.long")}</option>
            </select>
          </label>
          <label className="flex items-center gap-1 text-sm">
            {t("tts.summary.detail")}
            <select
              className={selectClass}
              value={detail}
              onChange={(e) => setDetail(e.target.value)}
            >
              <option value="ueberblick">{t("tts.summary.details.overview")}</option>
              <option value="ausgewogen">{t("tts.summary.details.balanced")}</option>
              <option value="detailliert">{t("tts.summary.details.deep")}</option>
            </select>
          </label>
          <label className="flex items-center gap-1 text-sm">
            {t("tts.summary.audience")}
            <select
              className={selectClass}
              value={audience}
              onChange={(e) => setAudience(e.target.value)}
            >
              <option value="allgemein">{t("tts.summary.audiences.general")}</option>
              <option value="fachpublikum">{t("tts.summary.audiences.expert")}</option>
              <option value="management">{t("tts.summary.audiences.management")}</option>
              <option value="einfache_sprache">{t("tts.summary.audiences.plain")}</option>
            </select>
          </label>
        </div>

        <div className="flex gap-2 items-center">
          <Button
            onClick={summarize}
            disabled={busy || source.trim().length === 0}
          >
            {t("tts.summary.run")}
          </Button>
          {busy && <Badge variant="secondary">{t("tts.summary.working")}</Badge>}
        </div>

        {summary && (
          <div className="space-y-2">
            <Textarea
              value={summary}
              onChange={(e) => setSummary(e.target.value)}
              rows={6}
              className="w-full"
            />
            <div className="flex gap-2 items-center flex-wrap">
              <Button onClick={speakSummary} disabled={busy}>
                {t("tts.summary.speak")}
              </Button>
              <Button variant="secondary" onClick={exportSummary} disabled={busy}>
                {t("tts.summary.export")}
              </Button>
              {saved && (
                <span className="text-xs text-text/60 break-all">
                  {t("tts.summary.savedTo", { path: saved })}
                </span>
              )}
            </div>
          </div>
        )}

        <p className="text-xs text-text/50">{t("tts.summary.providerHint")}</p>
      </div>
    </SettingsGroup>
  );
};
