import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Pencil, ArrowLeft } from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { writeTextFile } from "@tauri-apps/plugin-fs";
import { commands, type Meeting, type StoredSegment } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import { Textarea } from "../../ui/Textarea";
import { AudioPlayer, AudioPlayerGroup } from "../../ui/AudioPlayer";
import Badge from "../../ui/Badge";
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

const fileBaseName = (path: string) => path.split(/[\/]/).pop() ?? path;

interface MeetingDetailProps {
  meeting: Meeting;
  onBack: () => void;
}

export const MeetingDetail: React.FC<MeetingDetailProps> = ({
  meeting,
  onBack,
}) => {
  const { t, i18n } = useTranslation();
  const meetingId = meeting.id;
  const meetingTitle = meeting.title;
  const [tab, setTab] = useState<Tab>("transcript");
  const [segments, setSegments] = useState<StoredSegment[]>([]);
  const [loading, setLoading] = useState(true);
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const [copied, setCopied] = useState(false);
  const [transcriptError, setTranscriptError] = useState<string | null>(null);
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

  const transcriptText = () =>
    segments
      .map(
        (s) =>
          `${t(channelLabelKey(s.channel))} [${formatMmSs(s.start_ms)}]: ${s.text}`,
      )
      .join("\n");

  const copyTranscript = async () => {
    try {
      await navigator.clipboard.writeText(transcriptText());
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      setTranscriptError(String(e));
    }
  };

  const exportTranscript = async () => {
    setTranscriptError(null);
    try {
      const target = await save({
        defaultPath: `${meetingTitle || "transkript"}.txt`,
        filters: [{ name: "Text", extensions: ["txt", "md"] }],
      });
      if (!target) return;
      await writeTextFile(target, transcriptText());
    } catch (e) {
      setTranscriptError(String(e));
    }
  };

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

        {(meeting.mic_audio_path || meeting.system_audio_path) && (
          <AudioPlayerGroup>
            {meeting.mic_audio_path && (
              <div className="space-y-1">
                <p className="text-xs text-text/60">
                  {meeting.source === "import"
                    ? t("meetings.meta.audioImport")
                    : t("meetings.live.me")}
                  {" · "}
                  {fileBaseName(meeting.mic_audio_path)}
                </p>
                <AudioPlayer
                  src={convertFileSrc(meeting.mic_audio_path, "asset")}
                  className="w-full"
                />
              </div>
            )}
            {meeting.system_audio_path && (
              <div className="space-y-1">
                <p className="text-xs text-text/60">
                  {t("meetings.live.remote")}
                  {" · "}
                  {fileBaseName(meeting.system_audio_path)}
                </p>
                <AudioPlayer
                  src={convertFileSrc(meeting.system_audio_path, "asset")}
                  className="w-full"
                />
              </div>
            )}
          </AudioPlayerGroup>
        )}

        <div className="grid grid-cols-2 gap-x-4 gap-y-1 text-sm border border-mid-gray/20 rounded-md px-3 py-2">
          <span className="text-text/60">{t("meetings.meta.status")}</span>
          <span>
            <Badge
              variant={meeting.status === "ready" ? "success" : "secondary"}
            >
              {t(`meetings.status.${meeting.status}`, {
                defaultValue: meeting.status,
              })}
            </Badge>
          </span>
          <span className="text-text/60">{t("meetings.meta.source")}</span>
          <span>
            {t(`meetings.meta.sourceKind.${meeting.source}`, {
              defaultValue: meeting.source,
            })}
          </span>
          <span className="text-text/60">{t("meetings.meta.started")}</span>
          <span>
            {new Intl.DateTimeFormat(i18n.language, {
              dateStyle: "medium",
              timeStyle: "short",
            }).format(
              new Date((meeting.started_at ?? meeting.created_at) * 1000),
            )}
          </span>
          {meeting.duration_ms !== null && (
            <>
              <span className="text-text/60">
                {t("meetings.meta.duration")}
              </span>
              <span>{formatMmSs(meeting.duration_ms)}</span>
            </>
          )}
          {meeting.consent_confirmed_at !== null && (
            <>
              <span className="text-text/60">{t("meetings.meta.consent")}</span>
              <span>
                {new Intl.DateTimeFormat(i18n.language, {
                  dateStyle: "medium",
                  timeStyle: "short",
                }).format(new Date(meeting.consent_confirmed_at * 1000))}
              </span>
            </>
          )}
          {meeting.audio_retention_until !== null && (
            <>
              <span className="text-text/60">
                {t("meetings.meta.retentionUntil")}
              </span>
              <span>
                {new Intl.DateTimeFormat(i18n.language, {
                  dateStyle: "medium",
                  timeStyle: "short",
                }).format(new Date(meeting.audio_retention_until * 1000))}
              </span>
            </>
          )}
          <span className="text-text/60">{t("meetings.meta.segments")}</span>
          <span>{segments.length}</span>
        </div>

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

        {tab === "transcript" && segments.length > 0 && (
          <div className="flex items-center gap-2">
            <Button variant="secondary" size="sm" onClick={copyTranscript}>
              {copied
                ? t("meetings.detail.copied")
                : t("meetings.detail.copyTranscript")}
            </Button>
            <Button variant="secondary" size="sm" onClick={exportTranscript}>
              {t("meetings.detail.exportTranscript")}
            </Button>
          </div>
        )}
        {tab === "transcript" && transcriptError && (
          <p className="text-sm text-red-400">{transcriptError}</p>
        )}
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
