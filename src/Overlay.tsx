import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Hud } from "./api";
import { systemLang, translator, type Lang } from "./i18n";

const isMac = navigator.userAgent.includes("Mac");

/** Click-through pill near the bottom of the screen. Never takes focus. */
export function Overlay() {
  const [hud, setHud] = useState<Hud>({ state: "idle", code: null, level: 0, seconds: 0, lang: null });
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => {
    const un = listen<Hud>("hud", (e) => {
      window.clearTimeout(timer.current);
      setHud(e.payload);
      if (e.payload.state === "error" || e.payload.state === "copied") {
        timer.current = window.setTimeout(
          () => setHud((h) => ({ ...h, state: "idle" })),
          e.payload.state === "error" ? 3500 : 3000,
        );
      }
    });
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const lang = (hud.lang as Lang | null) ?? systemLang();
  const { t, err } = useMemo(() => translator(lang), [lang]);
  const visible = hud.state !== "idle";

  return (
    <div className={`hud ${visible ? "show" : ""} ${hud.state}`} aria-live="polite">
      {hud.state === "recording" ? (
        <>
          <span className="dot" />
          <span>{t("hudListening")}</span>
          <Meter level={hud.level} />
          <span className="time">
            {Math.floor(hud.seconds / 60)}:{String(hud.seconds % 60).padStart(2, "0")}
          </span>
        </>
      ) : hud.state === "transcribing" ? (
        <>
          <span className="spinner" />
          <span>{t("hudTranscribing")}</span>
        </>
      ) : hud.state === "copied" ? (
        <span>{t("hudCopied", { paste: isMac ? "⌘V" : "Ctrl + V" })}</span>
      ) : hud.state === "error" ? (
        <span>{err(hud.code)}</span>
      ) : null}
    </div>
  );
}

function Meter({ level }: { level: number }) {
  const v = Math.min(1, Math.sqrt(level / 300));
  return (
    <span className="meter" aria-hidden>
      {[0.5, 0.8, 1, 0.8, 0.5].map((w, i) => (
        <i key={i} style={{ transform: `scaleY(${0.2 + 0.8 * v * w})` }} />
      ))}
    </span>
  );
}
