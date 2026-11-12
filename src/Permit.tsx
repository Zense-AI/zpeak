import { useEffect, useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, type View } from "./api";
import { systemLang, translator, type Lang } from "./i18n";
import logo from "./app-icon.png";

const PREVIEW: View = {
  settings: {
    provider: null,
    shortcut: "Alt+Z",
    mode: "toggle",
    language: "auto",
    uiLanguage: null,
    localModel: "small",
    openrouterModel: null,
    restoreClipboard: true,
  },
  platform: "macos",
  version: "0.1.1",
  zenseSignedIn: false,
  openrouterKeySet: false,
  models: [],
  localSupported: true,
  downloading: null,
  accessibility: false,
};

/** First-run and “settings is closed” prompt for macOS Accessibility. */
export function PermitApp() {
  const [view, setView] = useState<View | null>(null);
  const [ready, setReady] = useState(false);
  const [opening, setOpening] = useState(false);
  const [preview, setPreview] = useState(false);

  useEffect(() => {
    if (ready) return;
    let stop = false;
    const tick = async () => {
      try {
        const next = await api.state();
        if (stop) return;
        setPreview(false);
        setView(next);
        if (next.accessibility) setReady(true);
      } catch {
        if (stop) return;
        setPreview(true);
        setView(PREVIEW);
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), 800);
    return () => {
      stop = true;
      window.clearInterval(id);
    };
  }, [ready]);

  useEffect(() => {
    if (!ready || preview) return;
    const id = window.setTimeout(() => {
      void api.finishAccessibilityPrompt();
    }, 900);
    return () => window.clearTimeout(id);
  }, [ready, preview]);

  const lang: Lang = (view?.settings.uiLanguage as Lang | null) ?? systemLang();
  const { t } = useMemo(() => translator(lang), [lang]);

  useEffect(() => {
    document.documentElement.lang = lang === "en" ? "en" : lang === "zh-hans" ? "zh-CN" : "zh-HK";
  }, [lang]);

  if (!view) return null;

  return (
    <main className="permit" role="dialog" aria-labelledby="permit-title">
      <img src={logo} alt="" className="permit-logo enter-in" />
      <h1 id="permit-title" className="enter-in" style={{ animationDelay: "40ms" }}>
        {ready ? t("accessibilityReady") : t("accessibilityTitle")}
      </h1>
      {ready ? null : (
        <p className="permit-body enter-in" style={{ animationDelay: "80ms" }}>
          {t("accessibilityBody")}
        </p>
      )}

      {ready ? (
        <div className="permit-ready enter-in" style={{ animationDelay: "120ms" }} aria-hidden>
          <span className="permit-check" />
        </div>
      ) : (
        <div className="permit-diagram enter-in" style={{ animationDelay: "120ms" }} aria-hidden>
          <p className="permit-kicker">{t("accessibilityPath")}</p>
          <div className="permit-group">
            <div className="permit-row muted">{t("accessibilityRow")}</div>
            <div className="permit-row">
              <img src={logo} alt="" className="permit-row-icon" />
              <span className="permit-row-copy">
                <span>Zpeak</span>
                <span className="muted small">{t("accessibilityTurnOn")}</span>
              </span>
              <span className="permit-switch" />
            </div>
          </div>
        </div>
      )}

      {ready ? null : (
        <div className="permit-actions enter-in" style={{ animationDelay: "160ms" }}>
          <button
            type="button"
            className="btn primary permit-cta"
            disabled={opening}
            onClick={async () => {
              if (preview) return;
              setOpening(true);
              try {
                const granted = await api.requestAccessibility();
                if (!granted) {
                  await openUrl(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                  );
                }
              } finally {
                setOpening(false);
              }
            }}
          >
            {t("accessibilityOpen")}
          </button>
          <button
            type="button"
            className="link permit-later"
            onClick={() => {
              if (!preview) void api.dismissAccessibilityPrompt();
            }}
          >
            {t("accessibilityLater")}
          </button>
        </div>
      )}
    </main>
  );
}
