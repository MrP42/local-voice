import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Pencil, ArrowLeft } from "lucide-react";
import { commands, type StoredSegment } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import { Textarea } from "../../ui/Textarea";
import { MinutesView } from "./MinutesView";

const formatMmSs = (ms: number) => {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
};

const channelLabelKey = (channel: number) => {
  switch (channel) {
    case 0:
      return "meetings.live.me";
    case 1:
      return "meetings.live.remote";
    default:
      return "meetings.live.mixed";
  }
};

type Tab = "transcript" | "minutes";

interface MeetingDetailProps {
  meetingId: string;
  meetingTitle: string;
  onBack: () => void;
}

export const MeetingDetail: React.FC<MeetingDetailProps> = ({
  meetingId,
  meetingTitle,
  onBack,
}) => {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>("transcript");
  const [segments, setSegments] = useState<StoredSegment[]>([]);
  const [loading, setLoading] = useState(true);
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [editText, setEditText] = useState("");
  const [saving, setSaving] = useState(false);

  const loadSegments = useCallback(async () => {
    setLoading(true);
    const result = await commands.meetingsGetSegments(meetingId);
    setLoading(false);
    if (result.status === "ok") {
      // Segments come back in segment_index order, which interleaves
      // channels for a live-recorded meeting — always sort by start_ms.
      setSegments([...result.data].sort((a, b) => a.start_ms - b.start_ms));
    }
  }, [meetingId]);

  useEffect(() => {
    void loadSegments();
  }, [loadSegments]);

  const startEdit = (segment: StoredSegment) => {
    setEditingIndex(segment.segment_index);
    setEditText(segment.text);
  };

  const cancelEdit = () => {
    setEditingIndex(null);
    setEditText("");
  };

  const saveEdit = async (segmentIndex: number) => {
    setSaving(true);
    const result = await commands.meetingsUpdateSegment(
      meetingId,
      segmentIndex,
      editText,
    );
    setSaving(false);
    if (result.status === "ok") {
      setSegments((prev) =>
        prev.map((s) =>
          s.segment_index === segmentIndex ? { ...s, text: editText } : s,
        ),
      );
      setEditingIndex(null);
      setEditText("");
    }
  };

  return (
    <SettingsGroup>
      <div className="px-4 py-3 space-y-3">
        <div className="flex items-center justify-between gap-2">
          <button
            type="button"
            onClick={onBack}
            className="flex items-center gap-1 text-sm text-text/70 hover:text-text cursor-pointer"
          >
            <ArrowLeft width={16} height={16} />
            {t("meetings.detail.back")}
          </button>
        </div>
        <h3 className="text-base font-semibold truncate">{meetingTitle}</h3>

        <div className="flex gap-1 border-b border-mid-gray/20">
          <button
            type="button"
            onClick={() => setTab("transcript")}
            className={`px-3 py-1.5 text-sm font-medium border-b-2 cursor-pointer ${
              tab === "transcript"
                ? "border-logo-primary text-text"
                : "border-transparent text-text/60 hover:text-text"
            }`}
          >
            {t("meetings.detail.transcriptTab")}
          </button>
          <button
            type="button"
            onClick={() => setTab("minutes")}
            className={`px-3 py-1.5 text-sm font-medium border-b-2 cursor-pointer ${
              tab === "minutes"
                ? "border-logo-primary text-text"
                : "border-transparent text-text/60 hover:text-text"
            }`}
          >
            {t("meetings.detail.minutesTab")}
          </button>
        </div>

        {tab === "transcript" ? (
          loading ? (
            <p className="text-sm text-text/60 text-center py-3">
              {t("meetings.list.loading")}
            </p>
          ) : segments.length === 0 ? (
            <p className="text-sm text-text/60">{t("meetings.live.empty")}</p>
          ) : (
            <div className="space-y-2 max-h-96 overflow-y-auto">
              {segments.map((segment) => (
                <div key={segment.segment_index} className="flex gap-2 items-start text-sm group">
                  <span className="text-xs text-text/40 w-10 shrink-0 pt-0.5">
                    {formatMmSs(segment.start_ms)}
                  </span>
                  <span className="text-xs text-text/50 w-16 shrink-0 pt-0.5">
                    {t(channelLabelKey(segment.channel))}
                  </span>
                  {editingIndex === segment.segment_index ? (
                    <div className="flex-1 space-y-1">
                      <Textarea
                        value={editText}
                        onChange={(e) => setEditText(e.target.value)}
                        rows={2}
                        className="w-full"
                        autoFocus
                      />
                      <div className="flex gap-2">
                        <Button
                          size="sm"
                          onClick={() => saveEdit(segment.segment_index)}
                          disabled={saving}
                        >
                          {t("meetings.detail.save")}
                        </Button>
                        <Button size="sm" variant="secondary" onClick={cancelEdit}>
                          {t("meetings.detail.cancel")}
                        </Button>
                      </div>
                    </div>
                  ) : (
                    <>
                      <p className="text-text/90 break-words flex-1">
                        {segment.text}
                      </p>
                      <button
                        type="button"
                        onClick={() => startEdit(segment)}
                        title={t("meetings.detail.editSegment")}
                        className="opacity-0 group-hover:opacity-100 p-1 rounded-md text-text/50 hover:text-logo-primary cursor-pointer shrink-0"
                      >
                        <Pencil width={14} height={14} />
                      </button>
                    </>
                  )}
                </div>
              ))}
            </div>
          )
        ) : (
          <MinutesView meetingId={meetingId} meetingTitle={meetingTitle} />
        )}
      </div>
    </SettingsGroup>
  );
};
