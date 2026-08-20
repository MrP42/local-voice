import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { commands, type TtsStatus } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { ShortcutInput } from "../ShortcutInput";
import { VoicesCard } from "./VoicesCard";
import { TranslateCard } from "./TranslateCard";
import { VoiceChangerCard } from "./VoiceChangerCard";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { SettingContainer } from "../../ui/SettingContainer";
import { Input } from "../../ui/Input";
import { Textarea } from "../../ui/Textarea";
import { Button } from "../../ui/Button";
import Badge from "../../ui/Badge";
import { Dialog } from "../../ui/Dialog";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { Slider } from "../../ui/Slider";
import { Select } from "../../ui/Select";
import { ReadingCard } from "./ReadingCard";
import { SummaryCard } from "./SummaryCard";
import { usePersistentState } from "../../../hooks/usePersistentState";
import { save } from "@tauri-apps/plugin-dialog";
import { Glyph } from "../../ui/AudioPlayer";
import { Dices, Download, OctagonX, Server } from "lucide-react";

const badgeVariant = (
  phase: TtsStatus["phase"] | undefined,
): "primary" | "success" | "secondary" => {
  switch (phase) {
    case "ready":
    case "speaking":
      return "success";
    case "starting":
      return "primary";
    default:
      return "secondary";
  }
};

export const TtsSettings = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [status, setStatus] = useState<TtsStatus | null>(null);
  // The text you were about to have read out survives leaving the page —
  // losing a pasted article because you glanced at the model list is the
  // kind of loss nobody forgives.
  const [text, setText] = usePersistentState<string>("tts.text", "");
  const [startingSeconds, setStartingSeconds] = useState(0);
  const [lastError, setLastError] = useState<string | null>(null);
  /** Rueckmeldung des harten Beendens — was gefunden und beendet wurde. */
  const [killNotice, setKillNotice] = useState<string | null>(null);
  /** Offene Rueckfrage vor dem Beenden des Servers. */
  const [confirmStop, setConfirmStop] = useState(false);
  const [speakProgress, setSpeakProgress] = useState<{
    position: number;
    total: number;
  } | null>(null);
  const [currentSentence, setCurrentSentence] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [exportProgress, setExportProgress] = useState<{
    position: number;
    total: number;
  } | null>(null);
  const startingTimer = useRef<number | null>(null);

  useEffect(() => {
    commands.ttsServerStatus().then((r) => {
      if (r.status === "ok") setStatus(r.data);
    });
    const un = listen<TtsStatus>("tts-state-changed", (e) => {
      setStatus(e.payload);
      if (e.payload.phase === "error" && e.payload.message) {
        setLastError(e.payload.message);
      } else if (
        e.payload.phase === "ready" ||
        e.payload.phase === "speaking"
      ) {
        setLastError(null);
      }
    });
    const unExport = listen<{
      position: number;
      total: number;
      cancelled: boolean;
    }>("tts-export-progress", (e) => {
      const { position, total, cancelled } = e.payload;
      if (cancelled || (total > 0 && position >= total)) {
        setSaving(false);
        setExportProgress(null);
        return;
      }
      setExportProgress({ position, total });
    });
    const unExportError = listen<{ message: string }>(
      "tts-export-error",
      (e) => {
        setSaving(false);
        setExportProgress(null);
        setLastError(e.payload.message);
      },
    );
    const unProgress = listen<{ position: number; total: number }>(
      "tts-speak-progress",
      (e) => setSpeakProgress(e.payload),
    );
    const unSentence = listen<{ context: string; text: string }>(
      "tts-current-sentence",
      (e) => {
        if (e.payload.context === "speak") setCurrentSentence(e.payload.text);
      },
    );
    return () => {
      un.then((f) => f());
      unExport.then((f) => f());
      unExportError.then((f) => f());
      unProgress.then((f) => f());
      unSentence.then((f) => f());
    };
  }, []);

  // Sekundenzähler nur während des Serverstarts.
  useEffect(() => {
    if (status?.phase === "starting") {
      if (startingTimer.current === null) {
        setStartingSeconds(0);
        startingTimer.current = window.setInterval(
          () => setStartingSeconds((s) => s + 1),
          1000,
        );
      }
    } else if (startingTimer.current !== null) {
      window.clearInterval(startingTimer.current);
      startingTimer.current = null;
      setStartingSeconds(0);
    }
    return () => {
      if (startingTimer.current !== null) {
        window.clearInterval(startingTimer.current);
        startingTimer.current = null;
      }
    };
  }, [status?.phase]);

  const phase = status?.phase ?? "stopped";
  const speaking = phase === "speaking";
  const starting = phase === "starting";

  const speak = async () => {
    if (canResume) {
      await resumeSpeaking();
      return;
    }
    setLastError(null);
    setSpeakProgress(null);
    const result = await commands.ttsSpeakText(text);
    if (result.status === "error") setLastError(result.error);
  };

  /**
   * Stops playback outright — this is a cancel, not a suspend; "Fortsetzen"
   * restarts from the last fully spoken sentence.
   *
   * Deliberately NEVER disabled. It used to be gated on `speaking`, which is
   * derived from a phase event — and any event that put the phase back to
   * "ready" mid-playback (a server health check did exactly that) left the
   * only stop control greyed out while audio kept running. Cancelling when
   * nothing is playing costs nothing; being unable to cancel costs the user
   * their loudspeakers.
   */
  /** Pause: anhalten, Position behalten — Play setzt genau dort fort. */
  const pauseSpeaking = () => {
    void commands.ttsCancel();
  };

  /**
   * Stop: anhalten UND an den Anfang. Das ist der Unterschied zu Pause und der
   * einzige Grund, warum das Design-System beide nebeneinander erlaubt —
   * decken sie sich, gehoert Stopp weg.
   */
  const stopSpeaking = () => {
    void commands.ttsCancel();
    setSpeakProgress(null);
  };

  const resumeSpeaking = async () => {
    setLastError(null);
    const result = await commands.ttsSpeakResume();
    if (result.status === "error") setLastError(result.error);
  };

  /**
   * The whole text — speaker changes and all — written to one WAV instead of
   * only played. Goes through the same segmentation as playback, so the file
   * sounds like what you heard.
   */
  const saveSpokenAudio = async () => {
    setLastError(null);
    const target = await save({
      filters: [{ name: "WAV", extensions: ["wav"] }],
      defaultPath: "vorlesen.wav",
    });
    if (typeof target !== "string") return;
    setSaving(true);
    setExportProgress({ position: 0, total: 0 });
    // Returns at once; the run reports itself through tts-export-progress.
    const result = await commands.ttsSpeakToFile(text, target);
    if (result.status === "error") {
      setSaving(false);
      setExportProgress(null);
      setLastError(result.error);
    }
  };

  const cancelExport = () => {
    void commands.ttsExportCancel();
  };

  /** One sentence back or forward — the unit spoken text moves in. */
  const seekSentence = (delta: number) => {
    void commands.ttsSpeakSeek(delta);
  };

  const canResume =
    !speaking &&
    speakProgress !== null &&
    speakProgress.position < speakProgress.total;

  const startServer = async () => {
    setLastError(null);
    const result = await commands.ttsServerStart();
    if (result.status === "error") setLastError(result.error);
  };

  /**
   * Harter Ausweg: beendet, was auf dem TTS-Port lauscht, ohne vorher zu
   * fragen, ob es antwortet. Meldet zurueck, was gefunden wurde — "nichts
   * gefunden" ist ein Ergebnis und kein Fehler, deshalb steht es als Hinweis
   * und nicht als Fehlermeldung.
   */
  const killServer = async () => {
    setLastError(null);
    const result = await commands.ttsServerKill();
    if (result.status === "error") {
      setLastError(result.error);
      return;
    }
    setKillNotice(result.data);
    window.setTimeout(() => setKillNotice(null), 4000);
  };

  const stopServer = () => {
    void commands.ttsServerStop();
  };

  /**
   * Farbe des Serversymbols. Nur Farbe, kein zweites Symbol: die Form soll
   * ueber alle Zustaende gleich bleiben, damit man sie an derselben Stelle
   * wiederfindet — was sich aendert, ist der Zustand, nicht die Sache.
   *
   * Der Fehlerzustand blinkt als einziger. Er ist der einzige, der eine
   * Handlung verlangt, die nicht aufschiebbar ist.
   */
  const serverIconClass =
    phase === "starting"
      ? "text-yellow-400 animate-pulse"
      : phase === "error"
        ? "text-orange-500 animate-pulse"
        : phase === "stopped"
          ? "text-text/40"
          : "text-green-500";

  const serverTitle =
    phase === "stopped"
      ? t("tts.serverIconStart")
      : phase === "starting"
        ? t("tts.serverIconStarting")
        : phase === "error"
          ? (status?.message ?? t("tts.serverIconError"))
          : t("tts.serverIconStop");

  /**
   * Ein Klick tut, was im jeweiligen Zustand ansteht. Beim laufenden Server
   * ist das Beenden — und weil damit ein Modellstart von bis zu zwei Minuten
   * verfaellt und laufendes Vorlesen abbricht, wird vorher gefragt.
   */
  const onServerIconClick = () => {
    if (phase === "stopped" || phase === "error") {
      void startServer();
      return;
    }
    if (phase === "starting") return;
    setConfirmStop(true);
  };

  const showVramHint =
    starting && (startingSeconds >= 120 || status?.message === "vram");

  return (
    <div className="w-full space-y-6">
      <SettingsGroup title={t("tts.title")}>
        <SettingContainer
          title={t("tts.serverTitle")}
          description={t("tts.description")}
          grouped={true}
          layout="horizontal"
        >
          <div className="flex items-center gap-1">
            {/* Zustand und Bedienung sind EIN Element: die Farbe sagt, woran
                man ist, der Klick tut das, was in diesem Zustand ansteht.
                Grau = aus (starten), Gelb = faehrt hoch, Gruen = laeuft
                (beenden, nach Rueckfrage), Orange blinkend = Fehler. */}
            <button
              type="button"
              onClick={onServerIconClick}
              title={serverTitle}
              aria-label={serverTitle}
              className="p-1.5 rounded-md hover:bg-mid-gray/20 transition-colors cursor-pointer"
            >
              <Server
                width={18}
                height={18}
                className={serverIconClass}
                aria-hidden="true"
              />
            </button>
            {/* Der Notausgang. Bewusst IMMER sichtbar und nie gesperrt: sein
                Zweck ist ja gerade, dass die gemeldete Phase falsch sein kann
                — ein haengender Server meldet nichts mehr, haelt aber die
                Grafikkarte fest. Ein Notausgang, der von derselben Anzeige
                abhinge wie das Problem, waere keiner. */}
            <button
              type="button"
              onClick={killServer}
              title={t("tts.serverKillHint")}
              aria-label={t("tts.serverKill")}
              className="p-1.5 rounded-md text-red-400 hover:text-red-300 hover:bg-red-500/10 transition-colors cursor-pointer"
            >
              <OctagonX width={18} height={18} aria-hidden="true" />
            </button>
            <Badge variant={badgeVariant(phase)}>
              {t(`tts.status.${phase}`, { seconds: startingSeconds })}
            </Badge>
          </div>
        </SettingContainer>
        {killNotice && (
          <p className="px-4 pb-2 text-sm text-text/70">{killNotice}</p>
        )}
        {showVramHint && (
          <p className="px-4 pb-2 text-sm text-text/70">{t("tts.vramHint")}</p>
        )}
        {lastError && (
          <p className="px-4 pb-2 text-sm text-red-500 break-words">
            {lastError}
          </p>
        )}
        <div className="px-4 pb-4 space-y-2">
          <Textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={t("tts.inputPlaceholder")}
            rows={5}
            className="w-full"
          />
          <div className="flex gap-2 items-center flex-wrap">
            {/* Transport per design system: round glyph buttons, exactly one
                primary. Reading aloud is playback, so it gets the same family
                as every audio player in the app — not text buttons. */}
            {/* Vollstaendige Transportzeile nach Katalog: von der Mitte nach
                aussen — Hauptschalter, daneben die Satzspruenge; hinter dem
                Trenner die Aktionen, die die Wiedergabe nicht fortbewegen.
                Statt ±15 s stehen hier Saetze: vorgelesener Text ist satzweise
                aufgebaut, eine Sekundenmarke gibt es darin nicht. */}
            <div className="mediabar mediabar--start">
              <button
                type="button"
                className="mbtn"
                onClick={() => seekSentence(-1)}
                disabled={!canResume && !speaking}
                aria-label={t("tts.previousSentence")}
              >
                <Glyph name="prev" />
              </button>
              <button
                type="button"
                className="mbtn mbtn--primary mbtn--lg"
                onClick={speaking ? pauseSpeaking : speak}
                disabled={!speaking && text.trim().length === 0}
                aria-label={speaking ? t("tts.pause") : t("tts.speak")}
              >
                <Glyph name={speaking ? "pause" : "play"} />
              </button>
              <button
                type="button"
                className="mbtn"
                onClick={() => seekSentence(1)}
                disabled={!canResume && !speaking}
                aria-label={t("tts.nextSentence")}
              >
                <Glyph name="next" />
              </button>
              <span className="mediabar__sep" />
              <button
                type="button"
                className="mbtn"
                onClick={stopSpeaking}
                aria-label={t("tts.stop")}
              >
                <Glyph name="stop" />
              </button>
            </div>
            <Button
              variant="secondary"
              onClick={saveSpokenAudio}
              disabled={saving || text.trim().length === 0}
            >
              <Download width={14} height={14} />
              {saving ? t("tts.savingAudio") : t("tts.saveAudio")}
            </Button>
            {saving && (
              <div className="flex items-center gap-2">
                <div className="w-32 h-1.5 rounded-full bg-mid-gray/20 overflow-hidden">
                  <div
                    className="h-full bg-logo-primary transition-[width] duration-200"
                    style={{
                      width: exportProgress?.total
                        ? `${(exportProgress.position / exportProgress.total) * 100}%`
                        : "0%",
                    }}
                  />
                </div>
                <span className="text-xs text-text/60 tabular-nums">
                  {exportProgress?.total
                    ? t("tts.sentenceProgress", {
                        position: exportProgress.position,
                        total: exportProgress.total,
                      })
                    : t("tts.savingAudio")}
                </span>
                <button
                  type="button"
                  className="mbtn mbtn--sm"
                  onClick={cancelExport}
                  aria-label={t("tts.cancelExport")}
                >
                  <Glyph name="stop" />
                </button>
              </div>
            )}
            {speakProgress && (
              <span className="text-xs text-text/60">
                {t("tts.sentenceProgress", {
                  position: speakProgress.position,
                  total: speakProgress.total,
                })}
              </span>
            )}
          </div>
          {/* Sprecherwechsel sind eine Schreibregel, keine Einstellung — der
              Hinweis steht deshalb bei dem Feld, in das man ihn tippt. */}
          <p className="text-xs text-text/50">{t("tts.dialogHint")}</p>
          {speaking && currentSentence && (
            <p className="text-sm italic text-text/70 border-s-2 border-logo-primary ps-2">
              {currentSentence}
            </p>
          )}
        </div>
      </SettingsGroup>

      <ReadingCard />

      <SummaryCard />

      <VoicesCard />

      <TranslateCard />

      <VoiceChangerCard />

      <SettingsGroup title={t("tts.settingsTitle")}>
        <ShortcutInput shortcutId="speak_clipboard" grouped={true} />
        <Slider
          value={getSetting("tts_volume") ?? 1.0}
          onChange={(value) => updateSetting("tts_volume", value)}
          min={0}
          max={1}
          step={0.05}
          formatValue={(value) => `${Math.round(value * 100)}%`}
          label={t("tts.settings.volume")}
          description={t("tts.settings.volumeDescription")}
          grouped={true}
        />
        <ToggleSwitch
          checked={getSetting("tts_normalize") ?? true}
          onChange={(checked) => updateSetting("tts_normalize", checked)}
          isUpdating={isUpdating("tts_normalize")}
          label={t("tts.settings.normalize")}
          description={t("tts.settings.normalizeDescription")}
          grouped={true}
        />
        <Slider
          value={getSetting("tts_speed") ?? 1.0}
          onChange={(value) => updateSetting("tts_speed", value)}
          min={0.5}
          max={2}
          step={0.05}
          formatValue={(value) => `${value.toFixed(2)}×`}
          label={t("tts.settings.speed")}
          description={t("tts.settings.speedDescription")}
          grouped={true}
        />
        <SettingContainer
          title={t("tts.settings.exportFormat")}
          description={t("tts.settings.exportFormatDescription")}
          grouped={true}
          layout="horizontal"
        >
          <div className="w-36">
            {/* Formatnamen sind Eigennamen — bewusst nicht übersetzt. */}
            <Select
              value={getSetting("tts_export_format") ?? "wav"}
              options={[
                { value: "wav", label: "WAV" },
                { value: "mp3", label: "MP3" },
                { value: "opus", label: "Opus" },
              ]}
              isClearable={false}
              onChange={(value) => {
                if (value) updateSetting("tts_export_format", value);
              }}
            />
          </div>
        </SettingContainer>
        <SettingContainer
          title={t("tts.settings.fishDir")}
          description={t("tts.settings.fishDirDescription")}
          grouped={true}
          layout="stacked"
        >
          <Input
            type="text"
            value={getSetting("tts_fish_dir") ?? ""}
            onChange={(e) => updateSetting("tts_fish_dir", e.target.value)}
            disabled={isUpdating("tts_fish_dir")}
            className="w-full"
          />
        </SettingContainer>
        <SettingContainer
          title={t("tts.settings.port")}
          description={t("tts.settings.portDescription")}
          grouped={true}
          layout="horizontal"
        >
          <Input
            type="number"
            min="1"
            max="65535"
            value={getSetting("tts_port") ?? 8080}
            onChange={(e) => {
              const value = parseInt(e.target.value, 10);
              if (!isNaN(value) && value > 0 && value <= 65535) {
                updateSetting("tts_port", value);
              }
            }}
            disabled={isUpdating("tts_port")}
            className="w-24"
          />
        </SettingContainer>
        <SettingContainer
          title={t("tts.settings.seed")}
          description={t("tts.settings.seedDescription")}
          grouped={true}
          layout="horizontal"
        >
          {/* Der Seed bestimmt, wie die Standardstimme klingt. Er ist fest
              einstellbar, damit eine gefundene Stimme wiederholbar bleibt —
              und wuerfelbar, weil man sie nur durch Ausprobieren findet. Der
              gewuerfelte Wert landet sichtbar im Feld; genau der ist die
              Notiz, mit der man spaeter zurueckkommt. */}
          <div className="flex items-center gap-2">
            <Input
              type="number"
              value={getSetting("tts_seed") ?? 42}
              onChange={(e) => {
                const value = parseInt(e.target.value, 10);
                if (!isNaN(value)) updateSetting("tts_seed", value);
              }}
              disabled={isUpdating("tts_seed")}
              className="w-28"
            />
            <Button
              variant="secondary"
              size="sm"
              onClick={() =>
                updateSetting(
                  "tts_seed",
                  Math.floor(Math.random() * 2_147_483_647) + 1,
                )
              }
              disabled={isUpdating("tts_seed")}
            >
              <Dices width={14} height={14} />
              {t("tts.settings.rollSeed")}
            </Button>
          </div>
        </SettingContainer>
        <SettingContainer
          title={t("tts.settings.idleMinutes")}
          description={t("tts.settings.idleMinutesDescription")}
          grouped={true}
          layout="horizontal"
        >
          <Input
            type="number"
            min="0"
            max="1440"
            value={getSetting("tts_idle_minutes") ?? 15}
            onChange={(e) => {
              const value = parseInt(e.target.value, 10);
              if (!isNaN(value) && value >= 0) {
                updateSetting("tts_idle_minutes", value);
              }
            }}
            disabled={isUpdating("tts_idle_minutes")}
            className="w-24"
          />
        </SettingContainer>
        <ToggleSwitch
          checked={getSetting("tts_compile") ?? true}
          onChange={(checked) => updateSetting("tts_compile", checked)}
          isUpdating={isUpdating("tts_compile")}
          label={t("tts.settings.compile")}
          description={t("tts.settings.compileDescription")}
          grouped={true}
        />
        <ToggleSwitch
          checked={getSetting("tts_context_menu") ?? false}
          onChange={(checked) => updateSetting("tts_context_menu", checked)}
          isUpdating={isUpdating("tts_context_menu")}
          label={t("tts.settings.contextMenu")}
          description={t("tts.settings.contextMenuDescription")}
          grouped={true}
        />
        <SettingContainer
          title={t("tts.settings.maxChars")}
          description={t("tts.settings.maxCharsDescription")}
          grouped={true}
          layout="horizontal"
        >
          <Input
            type="number"
            min="100"
            max="100000"
            value={getSetting("tts_max_chars") ?? 5000}
            onChange={(e) => {
              const value = parseInt(e.target.value, 10);
              if (!isNaN(value) && value >= 100) {
                updateSetting("tts_max_chars", value);
              }
            }}
            disabled={isUpdating("tts_max_chars")}
            className="w-24"
          />
        </SettingContainer>
      </SettingsGroup>

      <Dialog
        open={confirmStop}
        onOpenChange={setConfirmStop}
        title={t("tts.stopConfirmTitle")}
        closeLabel={t("tts.stopConfirmCancel")}
        footer={
          <>
            <Button variant="secondary" onClick={() => setConfirmStop(false)}>
              {t("tts.stopConfirmCancel")}
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                setConfirmStop(false);
                stopServer();
              }}
            >
              {t("tts.stopConfirmAccept")}
            </Button>
          </>
        }
      >
        <p className="text-sm text-text/80">{t("tts.stopConfirmBody")}</p>
      </Dialog>
    </div>
  );
};
