import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { commands, type ReadingInfo } from "@/bindings";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { Button } from "../../ui/Button";
import Badge from "../../ui/Badge";

const percent = (info: ReadingInfo) =>
  info.total > 0 ? Math.round((info.position / info.total) * 100) : 0;

export const ReadingCard = () => {
  const { t } = useTranslation();
  const [current, setCurrent] = useState<ReadingInfo | null>(null);
  const [library, setLibrary] = useState<ReadingInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refreshLibrary = useCallback(async () => {
    const result = await commands.ttsReadingList();
    if (result.status === "ok") setLibrary(result.data);
  }, []);

  useEffect(() => {
    void refreshLibrary();
    const un = listen<ReadingInfo>("tts-reading-progress", (e) => {
      setCurrent((prev) =>
        prev === null || prev.key === e.payload.key ? e.payload : prev,
      );
      setLibrary((prev) =>
        prev.some((d) => d.key === e.payload.key)
          ? prev.map((d) =>
              d.key === e.payload.key ? { ...e.payload, playing: false } : d,
            )
          : [...prev, { ...e.payload, playing: false }],
      );
    });
    return () => {
      un.then((f) => f());
    };
  }, [refreshLibrary]);

  const openDocument = useCallback(
    async (path?: string) => {
      setError(null);
      let file = path;
      if (!file) {
        const picked = await open({
          multiple: false,
          filters: [
            { name: "Dokumente", extensions: ["txt", "md", "pdf", "docx"] },
          ],
        });
        if (typeof picked !== "string") return;
        file = picked;
      }
      setBusy(true);
      const result = await commands.ttsReadingOpen(file);
      setBusy(false);
      if (result.status === "error") {
        setError(result.error);
        return;
      }
      setCurrent(result.data);
      await refreshLibrary();
    },
    [refreshLibrary],
  );

  // Drag & Drop: ein Dokument aufs Fenster ziehen lädt es in die Bibliothek.
  const [dragging, setDragging] = useState(false);
  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type === "enter") {
        setDragging(true);
      } else if (event.payload.type === "leave") {
        setDragging(false);
      } else if (event.payload.type === "drop") {
        setDragging(false);
        const doc = event.payload.paths.find((p) =>
          /\.(txt|md|pdf|docx)$/i.test(p),
        );
        if (doc) void openDocument(doc);
      }
    });
    return () => {
      un.then((f) => f());
    };
  }, [openDocument]);

  const play = async () => {
    setError(null);
    const result = await commands.ttsReadingPlay();
    if (result.status === "error") setError(result.error);
  };

  const pause = () => {
    void commands.ttsReadingPause();
  };

  const reset = async (key: string) => {
    await commands.ttsReadingReset(key);
    await refreshLibrary();
    if (current?.key === key) {
      setCurrent({ ...current, position: 0, finished: false, playing: false });
    }
  };

  const remove = async (key: string) => {
    await commands.ttsReadingRemove(key);
    await refreshLibrary();
    if (current?.key === key) setCurrent(null);
  };

  return (
    <SettingsGroup title={t("tts.reading.title")}>
      <div
        className={`px-4 py-3 space-y-3 rounded-lg transition-colors ${
          dragging ? "bg-logo-primary/10 outline-2 outline-dashed outline-logo-primary" : ""
        }`}
      >
        <p className="text-sm text-text/70">{t("tts.reading.description")}</p>
        <p className="text-xs text-text/50">{t("tts.reading.dropHint")}</p>
        {error && <p className="text-sm text-red-500 break-words">{error}</p>}

        <div className="flex gap-2 items-center">
          <Button onClick={() => openDocument()} disabled={busy}>
            {t("tts.reading.open")}
          </Button>
          {current && (
            <>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void commands.ttsReadingSeek(-1)}
                disabled={busy}
                title={t("tts.reading.back")}
              >
                {"◀"}
              </Button>
              {current.playing ? (
                <Button variant="secondary" onClick={pause}>
                  {t("tts.reading.pause")}
                </Button>
              ) : (
                <Button variant="secondary" onClick={play} disabled={busy}>
                  {current.position > 0
                    ? t("tts.reading.resume")
                    : t("tts.reading.play")}
                </Button>
              )}
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void commands.ttsReadingSeek(1)}
                disabled={busy}
                title={t("tts.reading.forward")}
              >
                {"▶"}
              </Button>
            </>
          )}
          {busy && <Badge variant="secondary">{t("tts.reading.working")}</Badge>}
        </div>

        {current && (
          <div className="space-y-1">
            <div className="flex items-center justify-between gap-2">
              <span className="text-sm font-medium truncate">
                {current.title}
              </span>
              <span className="text-xs text-text/60 whitespace-nowrap">
                {t("tts.reading.progress", {
                  position: current.position,
                  total: current.total,
                  percent: percent(current),
                })}
              </span>
            </div>
            <div className="w-full h-2 rounded-full bg-mid-gray/20 overflow-hidden">
              <div
                className="h-full rounded-full bg-logo-primary transition-[width] duration-300"
                style={{ width: `${percent(current)}%` }}
              />
            </div>
          </div>
        )}

        {library.length > 0 && (
          <div className="space-y-1 pt-1 border-t border-mid-gray/20">
            <p className="text-xs uppercase tracking-wide text-text/50 pt-1">
              {t("tts.reading.library")}
            </p>
            {library.map((doc) => (
              <div
                key={doc.key}
                className="flex items-center justify-between gap-2 py-1"
              >
                <div className="min-w-0">
                  <span className="text-sm truncate block" title={doc.key}>
                    {doc.title}
                  </span>
                  <span className="text-xs text-text/60">
                    {doc.finished
                      ? t("tts.reading.finished")
                      : t("tts.reading.progress", {
                          position: doc.position,
                          total: doc.total,
                          percent: percent(doc),
                        })}
                  </span>
                </div>
                <div className="flex items-center gap-1 shrink-0">
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => openDocument(doc.key)}
                    disabled={busy}
                  >
                    {t("tts.reading.openEntry")}
                  </Button>
                  <Button size="sm" variant="ghost" onClick={() => reset(doc.key)}>
                    {t("tts.reading.reset")}
                  </Button>
                  <Button
                    size="sm"
                    variant="danger-ghost"
                    onClick={() => remove(doc.key)}
                  >
                    {t("tts.reading.remove")}
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </SettingsGroup>
  );
};
