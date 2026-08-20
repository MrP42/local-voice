import { useState } from "react";
import { useTranslation } from "react-i18next";
import { open, save } from "@tauri-apps/plugin-dialog";
import { commands } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Textarea } from "../../ui/Textarea";
import { Button } from "../../ui/Button";
import Badge from "../../ui/Badge";
import { Input } from "../../ui/Input";
import { Select } from "../../ui/Select";
import { Upload } from "lucide-react";
import {
  usePersistentNullableText,
  usePersistentState,
} from "../../../hooks/usePersistentState";

export const SummaryCard = () => {
  const { t } = useTranslation();
  // Everything the user put in or got out survives leaving the page. A summary
  // costs a model run and, with a local Ollama, minutes of it — throwing it
  // away because someone clicked "Modelle" is not a state worth returning to.
  // Deliberately NOT persisted: `error`, `busy`, `saved`. Those describe the
  // last attempt, not the work, and a stale "in progress" after a restart
  // would be a lie.
  const [source, setSource] = usePersistentState<string>(
    "tts.summary.source",
    "",
  );
  const [sourceName, setSourceName] = usePersistentNullableText(
    "tts.summary.sourceName",
  );
  const [summary, setSummary] = usePersistentState<string>(
    "tts.summary.text",
    "",
  );
  const [length, setLength] = usePersistentState<string>(
    "tts.summary.length",
    "mittel",
  );
  const [detail, setDetail] = usePersistentState<string>(
    "tts.summary.detail",
    "ausgewogen",
  );
  const [audience, setAudience] = usePersistentState<string>(
    "tts.summary.audience",
    "allgemein",
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [saved, setSaved] = useState<string | null>(null);

  const [url, setUrl] = usePersistentState<string>("tts.summary.url", "");

  const loadUrl = async () => {
    setError(null);
    setBusy(true);
    const result = await commands.ttsExtractUrl(url.trim());
    setBusy(false);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setSource(result.data);
    setSourceName(url.trim());
  };

  const loadDocument = async () => {
    setError(null);
    const picked = await open({
      multiple: false,
      filters: [
        { name: "Dokumente", extensions: ["txt", "md", "pdf", "docx"] },
      ],
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

  return (
    <SettingsGroup title={t("tts.summary.title")}>
      <div className="px-4 py-3 space-y-3">
        <p className="text-sm text-text/70">{t("tts.summary.description")}</p>
        {error && <p className="text-sm text-red-500 break-words">{error}</p>}

        <div className="flex gap-2 items-center flex-wrap">
          <Button variant="secondary" onClick={loadDocument} disabled={busy}>
            <Upload width={14} height={14} />
            {t("tts.summary.loadDocument")}
          </Button>
          <Input
            type="text"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder={t("tts.summary.urlPlaceholder")}
            className="flex-1 min-w-48"
          />
          <Button
            variant="secondary"
            onClick={loadUrl}
            disabled={busy || url.trim().length === 0}
          >
            {t("tts.summary.loadUrl")}
          </Button>
          {sourceName && (
            <span className="text-xs text-text/60 truncate w-full">
              {sourceName}
            </span>
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
            <div className="w-44">
              <Select
                value={length}
                isClearable={false}
                options={[
                  { value: "kurz", label: t("tts.summary.lengths.short") },
                  { value: "mittel", label: t("tts.summary.lengths.medium") },
                  { value: "lang", label: t("tts.summary.lengths.long") },
                ]}
                onChange={(value) => value && setLength(value)}
              />
            </div>
          </label>
          <label className="flex items-center gap-1 text-sm">
            {t("tts.summary.detail")}
            <div className="w-40">
              <Select
                value={detail}
                isClearable={false}
                options={[
                  {
                    value: "ueberblick",
                    label: t("tts.summary.details.overview"),
                  },
                  {
                    value: "ausgewogen",
                    label: t("tts.summary.details.balanced"),
                  },
                  {
                    value: "detailliert",
                    label: t("tts.summary.details.deep"),
                  },
                ]}
                onChange={(value) => value && setDetail(value)}
              />
            </div>
          </label>
          <label className="flex items-center gap-1 text-sm">
            {t("tts.summary.audience")}
            <div className="w-44">
              <Select
                value={audience}
                isClearable={false}
                options={[
                  {
                    value: "allgemein",
                    label: t("tts.summary.audiences.general"),
                  },
                  {
                    value: "fachpublikum",
                    label: t("tts.summary.audiences.expert"),
                  },
                  {
                    value: "management",
                    label: t("tts.summary.audiences.management"),
                  },
                  {
                    value: "einfache_sprache",
                    label: t("tts.summary.audiences.plain"),
                  },
                ]}
                onChange={(value) => value && setAudience(value)}
              />
            </div>
          </label>
        </div>

        <div className="flex gap-2 items-center">
          <Button
            onClick={summarize}
            disabled={busy || source.trim().length === 0}
          >
            {t("tts.summary.run")}
          </Button>
          {busy && (
            <Badge variant="secondary">{t("tts.summary.working")}</Badge>
          )}
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
              <Button
                variant="secondary"
                onClick={exportSummary}
                disabled={busy}
              >
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
