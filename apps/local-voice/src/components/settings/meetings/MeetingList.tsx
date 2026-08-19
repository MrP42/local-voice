import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { commands, events, type Meeting } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import { Dialog } from "../../ui/Dialog";
import { Alert } from "../../ui/Alert";
import Badge from "../../ui/Badge";
import { Trash2 } from "lucide-react";

const PAGE_SIZE = 25;

const statusBadgeVariant = (
  status: string,
): "primary" | "success" | "secondary" => {
  switch (status) {
    case "ready":
      return "success";
    case "recording":
      return "primary";
    default:
      return "secondary";
  }
};

const formatDuration = (durationMs: number | null) => {
  if (durationMs === null) return "--:--";
  const totalSeconds = Math.max(0, Math.floor(durationMs / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
};

interface MeetingListProps {
  onSelect: (meeting: Meeting) => void;
}

export const MeetingList: React.FC<MeetingListProps> = ({ onSelect }) => {
  const { t, i18n } = useTranslation();
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [loading, setLoading] = useState(true);
  const [hasMore, setHasMore] = useState(true);
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Meeting | null>(null);
  const sentinelRef = useRef<HTMLDivElement>(null);
  const loadingRef = useRef(false);

  const loadPage = useCallback(async (offset: number) => {
    const isFirstPage = offset === 0;
    if (!isFirstPage && loadingRef.current) return;
    loadingRef.current = true;
    if (isFirstPage) setLoading(true);

    try {
      const result = await commands.meetingsList(offset, PAGE_SIZE);
      if (result.status === "ok") {
        setMeetings((prev) =>
          isFirstPage ? result.data : [...prev, ...result.data],
        );
        setHasMore(result.data.length === PAGE_SIZE);
      }
    } finally {
      setLoading(false);
      loadingRef.current = false;
    }
  }, []);

  useEffect(() => {
    void loadPage(0);
  }, [loadPage]);

  // Refresh from the current recording/import/generation lifecycle. The
  // import path in particular emits no state events at all — its command
  // result is the only signal — so this only covers the recording/generate
  // paths; import success triggers its own explicit reload below.
  useEffect(() => {
    const un = events.meetingEvent.listen((e) => {
      if (e.payload.kind === "state") void loadPage(0);
    });
    return () => {
      un.then((f) => f());
    };
  }, [loadPage]);

  useEffect(() => {
    if (loading) return;
    const sentinel = sentinelRef.current;
    if (!sentinel || !hasMore) return;

    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting) {
          void loadPage(meetings.length);
        }
      },
      { threshold: 0 },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [loading, hasMore, loadPage, meetings.length]);

  const importFile = async () => {
    setImportError(null);
    const picked = await open({
      multiple: false,
      filters: [
        {
          name: "Media",
          extensions: [
            "wav",
            "mp3",
            "m4a",
            "mp4",
            "mkv",
            "mov",
            "flac",
            "ogg",
            "vtt",
            "srt",
          ],
        },
      ],
    });
    if (typeof picked !== "string") return;

    setImporting(true);
    const result = await commands.meetingsImportFile(picked, true);
    setImporting(false);
    if (result.status === "error") {
      setImportError(result.error);
      return;
    }
    // The import command's own return is the completion signal (no state
    // events fire for the synchronous VTT/SRT path) — refresh explicitly.
    void loadPage(0);
  };

  const confirmDelete = async () => {
    if (!deleteTarget) return;
    const id = deleteTarget.id;
    setDeleteTarget(null);
    setMeetings((prev) => prev.filter((m) => m.id !== id));
    const result = await commands.meetingsDelete(id);
    if (result.status === "error") void loadPage(0);
  };

  return (
    <SettingsGroup title={t("meetings.list.title")}>
      <div className="px-4 py-3 space-y-3">
        <div className="flex justify-between items-center gap-2">
          <p className="text-sm text-text/70">{t("meetings.list.title")}</p>
          <Button
            variant="secondary"
            size="sm"
            onClick={importFile}
            disabled={importing}
          >
            {t("meetings.list.import")}
          </Button>
        </div>
        {importError && <Alert variant="error">{importError}</Alert>}

        {loading ? (
          <p className="text-sm text-text/60 text-center py-3">
            {t("meetings.list.loading")}
          </p>
        ) : meetings.length === 0 ? (
          <p className="text-sm text-text/60 text-center py-3">
            {t("meetings.list.empty")}
          </p>
        ) : (
          <div className="divide-y divide-mid-gray/20">
            {meetings.map((meeting) => {
              const timestamp = meeting.started_at ?? meeting.created_at;
              const dateLabel = new Intl.DateTimeFormat(i18n.language, {
                year: "numeric",
                month: "short",
                day: "numeric",
                hour: "2-digit",
                minute: "2-digit",
              }).format(new Date(timestamp * 1000));

              return (
                <div
                  key={meeting.id}
                  className="flex items-center justify-between gap-2 py-2 cursor-pointer hover:bg-mid-gray/10 rounded-md px-1"
                  onClick={() => onSelect(meeting)}
                >
                  <div className="min-w-0">
                    <p className="text-sm font-medium truncate">
                      {meeting.title}
                    </p>
                    <p className="text-xs text-text/60">
                      {dateLabel} · {formatDuration(meeting.duration_ms)}
                    </p>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    <Badge variant={statusBadgeVariant(meeting.status)}>
                      {t(`meetings.status.${meeting.status}`, {
                        defaultValue: meeting.status,
                      })}
                    </Badge>
                    <button
                      type="button"
                      className="p-1.5 rounded-md text-text/50 hover:text-red-400 hover:bg-red-500/10 cursor-pointer"
                      title={t("meetings.list.deleteButton")}
                      onClick={(e) => {
                        e.stopPropagation();
                        setDeleteTarget(meeting);
                      }}
                    >
                      <Trash2 width={16} height={16} />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
        <div ref={sentinelRef} className="h-1" />
      </div>

      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
        title={t("meetings.list.deleteConfirmTitle")}
        closeLabel={t("meetings.list.cancel")}
        footer={
          <>
            <Button variant="secondary" onClick={() => setDeleteTarget(null)}>
              {t("meetings.list.cancel")}
            </Button>
            <Button variant="danger" onClick={confirmDelete}>
              {t("meetings.list.deleteButton")}
            </Button>
          </>
        }
      >
        <p className="text-sm text-text/80">{t("meetings.list.deleteConfirm")}</p>
      </Dialog>
    </SettingsGroup>
  );
};
