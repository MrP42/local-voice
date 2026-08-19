import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { save } from "@tauri-apps/plugin-dialog";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { commands, type MeetingDocument } from "@/bindings";
import { Button } from "../../ui/Button";
import { Alert } from "../../ui/Alert";
import Badge from "../../ui/Badge";
import { MarkdownContent } from "../../whats-new/MarkdownContent";

interface MinutesViewProps {
  meetingId: string;
  meetingTitle: string;
}

export const MinutesView: React.FC<MinutesViewProps> = ({
  meetingId,
  meetingTitle,
}) => {
  const { t } = useTranslation();
  const [doc, setDoc] = useState<MeetingDocument | null>(null);
  const [loading, setLoading] = useState(true);
  const [generating, setGenerating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState<string | null>(null);

  const loadLatest = useCallback(async () => {
    setLoading(true);
    const result = await commands.meetingsGetDocuments(meetingId);
    setLoading(false);
    if (result.status !== "ok") return;
    const minutes = result.data
      .filter((d) => d.kind === "minutes")
      .sort((a, b) => b.version - a.version);
    setDoc(minutes[0] ?? null);
  }, [meetingId]);

  useEffect(() => {
    void loadLatest();
  }, [loadLatest]);

  const generate = async () => {
    setGenerating(true);
    setError(null);
    setSaved(null);
    const result = await commands.meetingsGenerateMinutes(meetingId);
    setGenerating(false);
    if (result.status === "error") {
      setError(result.error);
      return;
    }
    setDoc(result.data);
  };

  const exportMinutes = async () => {
    if (!doc) return;
    setError(null);
    setSaved(null);
    const safeName = meetingTitle.replace(/[\\/:*?"<>|]/g, "_").trim() || "protokoll";
    const target = await save({
      filters: [{ name: "Markdown", extensions: ["md"] }],
      defaultPath: `${safeName}.md`,
    });
    if (typeof target !== "string") return;
    try {
      await writeTextFile(target, doc.body);
      setSaved(target);
    } catch (e) {
      setError(t("meetings.minutes.exportError") + `: ${String(e)}`);
    }
  };

  if (loading) {
    return (
      <p className="text-sm text-text/60 text-center py-3">
        {t("meetings.list.loading")}
      </p>
    );
  }

  return (
    <div className="space-y-3">
      {error && <Alert variant="error">{error}</Alert>}

      <div className="flex gap-2 items-center flex-wrap">
        <Button onClick={generate} disabled={generating}>
          {doc ? t("meetings.detail.regenerate") : t("meetings.detail.generate")}
        </Button>
        {doc && (
          <Button variant="secondary" onClick={exportMinutes}>
            {t("meetings.detail.export")}
          </Button>
        )}
        {generating && (
          <Badge variant="secondary">{t("meetings.minutes.generating")}</Badge>
        )}
        {saved && (
          <span className="text-xs text-text/60 break-all">
            {t("meetings.minutes.exportSaved", { path: saved })}
          </span>
        )}
      </div>

      {doc ? (
        <div className="rounded-lg border border-mid-gray/20 p-4">
          <MarkdownContent markdown={doc.body} />
        </div>
      ) : (
        !generating && (
          <p className="text-sm text-text/60">{t("meetings.minutes.empty")}</p>
        )
      )}
    </div>
  );
};
