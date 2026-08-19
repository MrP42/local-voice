import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Dialog } from "../../ui/Dialog";
import { Alert } from "../../ui/Alert";
import Badge from "../../ui/Badge";
import { translateMeetingError } from "./meetingErrors";

type Phase = "idle" | "recording" | "paused";

const LevelBar: React.FC<{ label: string; value: number }> = ({
  label,
  value,
}) => (
  <div className="flex items-center gap-2">
    <span className="text-xs text-text/60 w-20 shrink-0">{label}</span>
    <div className="h-1.5 flex-1 rounded-full bg-mid-gray/20 overflow-hidden">
      <div
        className="h-full rounded-full bg-logo-primary"
        style={{
          width: `${Math.min(100, Math.max(0, value * 100))}%`,
          transition: "width 80ms linear",
        }}
      />
    </div>
  </div>
);

export const RecorderCard: React.FC = () => {
  const { t } = useTranslation();
  const [title, setTitle] = useState("");
  const [captureSystem, setCaptureSystem] = useState(false);
  const [phase, setPhase] = useState<Phase>("idle");
  const [consentOpen, setConsentOpen] = useState(false);
  const [micLevel, setMicLevel] = useState(0);
  const [systemLevel, setSystemLevel] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    commands.meetingsIsRecording().then((r) => {
      if (r.status === "ok" && r.data) setPhase("recording");
    });

    const un = events.meetingEvent.listen((e) => {
      const payload = e.payload;
      if (payload.kind === "state") {
        if (payload.status === "recording" || payload.status === "paused") {
          setPhase(payload.paused ? "paused" : "recording");
        } else {
          setPhase("idle");
        }
      } else if (payload.kind === "levels") {
        setMicLevel(payload.mic);
        setSystemLevel(payload.system);
      } else if (payload.kind === "error") {
        setError(translateMeetingError(payload.message, t));
        setBusy(false);
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, []);

  const openConsent = () => {
    setError(null);
    setConsentOpen(true);
  };

  const confirmStart = async () => {
    setBusy(true);
    setError(null);
    const result = await commands.meetingsStart(
      title.trim() || t("meetings.record.titlePlaceholder"),
      true,
      captureSystem,
    );
    setBusy(false);
    setConsentOpen(false);
    if (result.status === "error") {
      setError(translateMeetingError(result.error, t));
      return;
    }
    setPhase("recording");
  };

  const pause = () => {
    void commands.meetingsPause();
  };

  const resume = () => {
    void commands.meetingsResume();
  };

  const stop = async () => {
    setBusy(true);
    const result = await commands.meetingsStop();
    setBusy(false);
    if (result.status === "error") {
      setError(translateMeetingError(result.error, t));
      return;
    }
    setPhase("idle");
    setTitle("");
    setMicLevel(0);
    setSystemLevel(0);
  };

  const recording = phase === "recording";
  const paused = phase === "paused";
  const active = recording || paused;

  return (
    <SettingsGroup title={t("meetings.title")}>
      <div className="px-4 py-3 space-y-3">
        {error && <Alert variant="error">{error}</Alert>}

        {!active && (
          <div className="flex gap-2 items-center flex-wrap">
            <Input
              type="text"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder={t("meetings.record.titlePlaceholder")}
              className="flex-1 min-w-48"
            />
            <label className="flex items-center gap-2 text-sm">
              <input
                type="checkbox"
                checked={captureSystem}
                onChange={(e) => setCaptureSystem(e.target.checked)}
                className="accent-logo-primary"
              />
              {t("meetings.record.captureSystem")}
            </label>
          </div>
        )}

        <div className="flex gap-2 items-center flex-wrap">
          {!active ? (
            <Button onClick={openConsent} disabled={busy}>
              {t("meetings.record.start")}
            </Button>
          ) : (
            <>
              <Badge variant={paused ? "secondary" : "success"}>
                {paused ? t("meetings.record.pause") : t("meetings.title")}
              </Badge>
              {paused ? (
                <Button variant="secondary" onClick={resume} disabled={busy}>
                  {t("meetings.record.resume")}
                </Button>
              ) : (
                <Button variant="secondary" onClick={pause} disabled={busy}>
                  {t("meetings.record.pause")}
                </Button>
              )}
              <Button variant="danger" onClick={stop} disabled={busy}>
                {t("meetings.record.stop")}
              </Button>
            </>
          )}
        </div>

        {active && (
          <div className="space-y-1.5 pt-1">
            <LevelBar label={t("meetings.record.micLevel")} value={micLevel} />
            {captureSystem && (
              <LevelBar
                label={t("meetings.record.systemLevel")}
                value={systemLevel}
              />
            )}
          </div>
        )}
      </div>

      <Dialog
        open={consentOpen}
        onOpenChange={setConsentOpen}
        title={t("meetings.consent.title")}
        closeLabel={t("meetings.consent.cancel")}
        footer={
          <>
            <Button
              variant="secondary"
              onClick={() => setConsentOpen(false)}
              disabled={busy}
            >
              {t("meetings.consent.cancel")}
            </Button>
            <Button onClick={confirmStart} disabled={busy}>
              {t("meetings.consent.confirm")}
            </Button>
          </>
        }
      >
        <p className="text-sm text-text/80 whitespace-pre-wrap">
          {t("meetings.consent.body")}
        </p>
      </Dialog>
    </SettingsGroup>
  );
};
