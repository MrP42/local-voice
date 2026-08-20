import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { save } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { commands, type MeetingDocument } from "@/bindings";
import { Button } from "../../ui/Button";
import { Alert } from "../../ui/Alert";
import Badge from "../../ui/Badge";
import { MarkdownContent } from "../../whats-new/MarkdownContent";
import { Download } from "lucide-react";

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
  const [autoFile, setAutoFile] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  // The rendered preview doubles as the source for the formatted clipboard
  // copy: reading its innerHTML guarantees that what lands in Word is exactly
  // what the user sees here, instead of a second Markdown renderer that could
  // drift from the first.
  const previewRef = useRef<HTMLDivElement>(null);

  const loadLatest = useCallback(async () => {
    setLoading(true);
    const result = await commands.meetingsGetDocuments(meetingId);
    setLoading(false);
    if (result.status !== "ok") {
      // Failing to load used to render as "no minutes yet" — which is exactly
      // what a deleted document looks like. Say which of the two it is.
      setError(t("meetings.minutes.loadError", { error: result.error }));
      return;
    }
    setError(null);
    const minutes = result.data
      .filter((d) => d.kind === "minutes")
      .sort((a, b) => b.version - a.version);
    setDoc(minutes[0] ?? null);
  }, [meetingId, t]);

  const refreshAutoFile = useCallback(async () => {
    const result = await commands.meetingsMinutesFile(meetingId);
    setAutoFile(result.status === "ok" ? result.data : null);
  }, [meetingId]);

  useEffect(() => {
    void loadLatest();
    void refreshAutoFile();
  }, [loadLatest, refreshAutoFile]);

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
    void refreshAutoFile();
  };

  /**
   * Two flavours in one clipboard write: `text/html` so a paste into Word,
   * Outlook or a mail client keeps headings and lists, and `text/plain` with
   * the Markdown source for editors that want it raw. The receiving
   * application picks whichever it understands.
   */
  const copyMinutes = async () => {
    if (!doc) return;
    setError(null);
    const html = previewRef.current?.innerHTML;
    try {
      if (html && typeof ClipboardItem !== "undefined") {
        await navigator.clipboard.write([
          new ClipboardItem({
            "text/html": new Blob([html], { type: "text/html" }),
            "text/plain": new Blob([doc.body], { type: "text/plain" }),
          }),
        ]);
      } else {
        // A webview without ClipboardItem still gets the Markdown.
        await navigator.clipboard.writeText(doc.body);
      }
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      setError(t("meetings.minutes.copyError") + `: ${String(e)}`);
    }
  };

  const exportMinutes = async () => {
    if (!doc) return;
    setError(null);
    setSaved(null);
    const safeName =
      meetingTitle.replace(/[\\/:*?"<>|]/g, "_").trim() || "protokoll";
    const target = await save({
      filters: [
        { name: "Markdown", extensions: ["md"] },
        { name: "Text", extensions: ["txt"] },
      ],
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
          {doc
            ? t("meetings.detail.regenerate")
            : t("meetings.detail.generate")}
        </Button>
        {doc && (
          <>
            <Button variant="secondary" onClick={copyMinutes}>
              {copied
                ? t("meetings.detail.copied")
                : t("meetings.minutes.copy")}
            </Button>
            <Button variant="secondary" onClick={exportMinutes}>
              <Download width={14} height={14} />
              {t("meetings.detail.export")}
            </Button>
          </>
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

      {/* Written automatically on generation, so the minutes exist as a file
          even for someone who never opens the export dialog. */}
      {doc && autoFile && (
        <p className="text-xs text-text/60 break-all">
          {t("meetings.minutes.autoSaved")}{" "}
          <button
            type="button"
            onClick={() => void revealItemInDir(autoFile)}
            className="underline hover:text-logo-primary cursor-pointer"
          >
            {autoFile}
          </button>
        </p>
      )}

      {doc ? (
        <div
          ref={previewRef}
          className="rounded-lg border border-mid-gray/20 p-4"
        >
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
