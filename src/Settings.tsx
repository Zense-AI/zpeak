import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, LazyMotion, MotionConfig, domAnimation, m } from "motion/react";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import {
  api,
  errorCode,
  type Profile,
  type Provider,
  type Settings,
  type SttModel,
  type View,
} from "./api";
import { systemLang, translator, type Lang, type T } from "./i18n";
import { acceleratorFromEvent, displayShortcut } from "./shortcut";
import logo from "./app-icon.png";
import zenseMark from "./logos/zense-mark.png";
import zenseMarkDark from "./logos/zense-mark-dark.png";
import openRouterMark from "./logos/openrouter-mark.png";

const LANGUAGES = ["auto", "en", "zh", "yue", "ja", "ko", "fr", "de", "es"] as const;

export function SettingsApp() {
  const [view, setView] = useState<View | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [menu, setMenu] = useState<null | "language" | "settings">(null);
  const actions = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menu) return;
    const onDown = (e: MouseEvent) => {
      if (!actions.current?.contains(e.target as Node)) setMenu(null);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenu(null);
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [menu]);

  const [panelHeight, setPanelHeight] = useState<number | "auto">("auto");
  const panelObserver = useMemo(
    () => new ResizeObserver(([e]) => setPanelHeight(e.borderBoxSize[0].blockSize)),
    [],
  );
  const panelRef = useCallback(
    (el: HTMLDivElement | null) => {
      panelObserver.disconnect();
      if (el) panelObserver.observe(el);
    },
    [panelObserver],
  );

  const refresh = useCallback(async () => setView(await api.state()), []);
  useEffect(() => {
    void refresh();
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [refresh]);

  const lang: Lang = (view?.settings.uiLanguage as Lang | null) ?? systemLang();
  const { t, err } = useMemo(() => translator(lang), [lang]);
  useEffect(() => {
    document.documentElement.lang = lang === "en" ? "en" : lang === "zh-hans" ? "zh-CN" : "zh-HK";
  }, [lang]);

  if (!view) return null;
  const isMac = view.platform === "macos";

  const save = async (patch: Partial<Settings>) => {
    const next = { ...view.settings, ...patch };
    setView({ ...view, settings: next });
    try {
      await api.saveSettings(next);
      setError(null);
    } catch (e) {
      setError(err(errorCode(e)));
      await refresh();
    }
  };

  const provider = view.settings.provider;
  const langLabel = lang === "en" ? "EN" : lang === "zh-hans" ? "简" : "繁";
  const shortcut = displayShortcut(view.settings.shortcut, isMac);
  const hold = view.settings.mode === "hold";

  return (
    <LazyMotion features={domAnimation} strict>
    <MotionConfig transition={EASE} reducedMotion="user">
    <main className="page">
      <header className="topbar">
        <div className="brand">
          <img src={logo} alt="" className="brand-mark" />
          <div>
            <h1>Zpeak</h1>
            <p className="muted small">
              {t(hold ? "speakHold" : "speakToggle", { shortcut })}
            </p>
          </div>
        </div>
        <div className="top-actions" ref={actions}>
          <button
            type="button"
            className="icon-btn"
            aria-expanded={menu === "language"}
            aria-label={t("languageTitle")}
            onClick={() => setMenu((m) => (m === "language" ? null : "language"))}
          >
            {langLabel}
          </button>
          <button
            type="button"
            className="icon-btn icon-only"
            aria-expanded={menu === "settings"}
            aria-label={t("settingsMenu")}
            onClick={() => setMenu((m) => (m === "settings" ? null : "settings"))}
          >
            <Gear />
          </button>
          <AnimatePresence>
          {menu === "language" ? (
            <m.div key="language" className="popover" role="menu" {...POP}>
              <label className="row">
                <span>{t("appLanguage")}</span>
                <select
                  value={view.settings.uiLanguage ?? ""}
                  onChange={(e) => save({ uiLanguage: e.target.value || null })}
                >
                  <option value="">{t("appLanguageSystem")}</option>
                  <option value="en">English</option>
                  <option value="zh-hant">繁體中文</option>
                  <option value="zh-hans">简体中文</option>
                </select>
              </label>
              <label className="row">
                <span>{t("spokenLanguage")}</span>
                <select
                  value={view.settings.language}
                  onChange={(e) => save({ language: e.target.value })}
                >
                  {LANGUAGES.map((l) => (
                    <option key={l} value={l}>
                      {t(`lang_${l}`)}
                    </option>
                  ))}
                </select>
              </label>
            </m.div>
          ) : null}
          {menu === "settings" ? (
            <m.div key="settings" className="popover" {...POP}>
              <ShortcutRow
                value={view.settings.shortcut}
                isMac={isMac}
                t={t}
                onChange={(shortcut) => save({ shortcut })}
              />
              <div className="segmented" role="radiogroup">
                {(["toggle", "hold"] as const).map((mode) => (
                  <button
                    key={mode}
                    type="button"
                    role="radio"
                    aria-checked={view.settings.mode === mode}
                    className={view.settings.mode === mode ? "on" : ""}
                    onClick={() => save({ mode })}
                  >
                    {mode === "toggle" ? t("modeToggle") : t("modeHold")}
                  </button>
                ))}
              </div>
              <div className="rows">
                <AutostartRow t={t} />
                <label className="row">
                  <span>{t("restoreClipboard")}</span>
                  <input
                    type="checkbox"
                    className="switch"
                    checked={view.settings.restoreClipboard}
                    onChange={(e) => save({ restoreClipboard: e.target.checked })}
                  />
                </label>
              </div>
              {isMac && !view.accessibility ? (
                <button
                  type="button"
                  className="btn primary"
                  onClick={async () => {
                    if (!(await api.requestAccessibility())) {
                      await openUrl(
                        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                      );
                    }
                  }}
                >
                  {t("accessibilityOpen")}
                </button>
              ) : null}
              <Footer version={view.version} t={t} />
            </m.div>
          ) : null}
          </AnimatePresence>
        </div>
      </header>

      {error ? (
        <p className="alert" role="alert">
          {error}
        </p>
      ) : null}

      <div className="listen">
      <div className="choices" role="radiogroup" aria-label={t("providerTitle")}>
        {(
          [
            ["zense", t("zenseTitle")],
            ["local", t("listenLocal")],
            ["openrouter", t("openrouterTitle")],
          ] as const
        ).map(([id, label], i) => (
          <m.button
            key={id}
            type="button"
            role="radio"
            aria-checked={provider === id}
            className={`choice ${provider === id ? "on" : ""}`}
            initial={{ opacity: 0, y: 8, scale: 0.97 }}
            animate={{ opacity: 1, y: 0, scale: 1, transition: { ...EASE, delay: i * 0.04 } }}
            whileTap={{ scale: 0.97 }}
            onClick={() => save({ provider: id })}
          >
            <ProviderLogo id={id} />
            <span>{label}</span>
          </m.button>
        ))}
      </div>

      <m.div className="card panel-card" initial={false} animate={{ height: panelHeight }}>
        <m.div
          key={provider ?? "none"}
          ref={panelRef}
          className="choice-panel"
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
        >
        {provider === "zense" ? (
          <>
            <p className="muted">{t("zenseDesc")}</p>
            <ZensePanel view={view} t={t} err={err} onChange={refresh} />
          </>
        ) : provider === "local" ? (
          <>
            <p className="muted">{t("localDesc")}</p>
            <LocalPanel view={view} t={t} err={err} onChange={refresh} save={save} />
          </>
        ) : provider === "openrouter" ? (
          <>
            <p className="muted">{t("openrouterDesc")}</p>
            <OpenRouterPanel view={view} t={t} err={err} onChange={refresh} save={save} />
          </>
        ) : (
          <p className="muted">{t("providerTitle")}</p>
        )}
        </m.div>
      </m.div>
      </div>

      <section className="try-dock">
        <h2>{t("tryTitle")}</h2>
        <textarea
          className="try"
          rows={3}
          placeholder={t(hold ? "tryPlaceholderHold" : "tryPlaceholder", { shortcut })}
        />
      </section>
    </main>
    </MotionConfig>
    </LazyMotion>
  );
}

const EASE = { duration: 0.28, ease: [0.23, 1, 0.32, 1] } as const;

const POP = {
  initial: { opacity: 0, y: -4, scale: 0.97 },
  animate: { opacity: 1, y: 0, scale: 1 },
  exit: { opacity: 0, y: -4, scale: 0.97, transition: { duration: 0.16 } },
} as const;

function Gear() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden>
      <path
        d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z"
        stroke="currentColor"
        strokeWidth="1.75"
      />
      <path
        d="M19.4 13.5a7.7 7.7 0 0 0 .1-1.5 7.7 7.7 0 0 0-.1-1.5l2-1.6-2-3.4-2.4 1a7.4 7.4 0 0 0-2.6-1.5L14 2h-4l-.4 2.5a7.4 7.4 0 0 0-2.6 1.5l-2.4-1-2 3.4 2 1.6a7.7 7.7 0 0 0 0 3l-2 1.6 2 3.4 2.4-1a7.4 7.4 0 0 0 2.6 1.5L10 22h4l.4-2.5a7.4 7.4 0 0 0 2.6-1.5l2.4 1 2-3.4-2-1.6Z"
        stroke="currentColor"
        strokeWidth="1.75"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function ProviderLogo({ id }: { id: Provider }) {
  if (id === "zense") {
    return (
      <span className="choice-logo">
        <img src={zenseMark} alt="" className="mark light" />
        <img src={zenseMarkDark} alt="" className="mark dark" />
      </span>
    );
  }
  if (id === "openrouter") {
    return (
      <span className="choice-logo">
        <img src={openRouterMark} alt="" className="mark" />
      </span>
    );
  }
  return (
    <span className="choice-logo device" aria-hidden>
      <svg viewBox="0 0 32 32" width="28" height="28">
        <rect x="4" y="6" width="24" height="16" rx="3" fill="none" stroke="currentColor" strokeWidth="1.75" />
        <path d="M12 26h8M16 22v4" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" />
        <path d="M10 14c1.2-2 2.2-2 3.2 0s2 2 3.2 0 2-2 3.2 0 2.2 2 3.4 0" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" />
      </svg>
    </span>
  );
}

type PanelProps = {
  view: View;
  t: T;
  err: (code: string | null) => string;
  onChange: () => Promise<void>;
};

function ZensePanel({ view, t, err, onChange }: PanelProps) {
  const [profile, setProfile] = useState<Profile | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const attempt = useRef(0);

  useEffect(() => {
    if (!view.zenseSignedIn) {
      setProfile(null);
      return;
    }
    api
      .zenseProfile()
      .then(setProfile)
      .catch((e) => {
        if (errorCode(e) === "SignedOut") void onChange();
      });
  }, [view.zenseSignedIn, onChange]);

  useEffect(() => {
    const un = listen("zense-signed-out", () => void onChange());
    return () => {
      void un.then((f) => f());
    };
  }, [onChange]);

  // Closing the browser tab sends nothing back, so coming back to Zpeak frees
  // the button. The browser flow keeps listening until a new attempt replaces it.
  useEffect(() => {
    if (!busy) return;
    const onFocus = () => {
      attempt.current++;
      setBusy(false);
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [busy]);

  const signIn = async () => {
    const id = ++attempt.current;
    setBusy(true);
    setMessage(null);
    try {
      const p = await api.zenseSignIn();
      setProfile(p);
      await onChange();
    } catch (e) {
      const code = errorCode(e);
      if (id === attempt.current && code !== "SignInAborted") setMessage(err(code));
    } finally {
      if (id === attempt.current) setBusy(false);
    }
  };

  if (view.zenseSignedIn && profile) {
    return (
      <div className="stack">
        <div className="row">
          <span>{t("zenseSignedInAs", { email: profile.email })}</span>
          <div className="row-actions">
            <button
              className="btn ghost"
              onClick={async () => {
                await api.zenseSignOut();
                setProfile(null);
                await onChange();
                await signIn();
              }}
            >
              {t("zenseSwitchAccount")}
            </button>
            <button
              className="btn ghost"
              onClick={async () => {
                await api.zenseSignOut();
                await onChange();
              }}
            >
              {t("zenseSignOut")}
            </button>
          </div>
        </div>
        {!profile.eligible ? <p className="warn">{t("zenseIneligible")}</p> : null}
      </div>
    );
  }

  return (
    <div className="stack">
      <button className="btn primary" disabled={busy} onClick={signIn}>
        {busy ? t("zenseSigningIn") : t("zenseSignIn")}
      </button>
      {message ? <p className="warn">{message}</p> : null}
    </div>
  );
}

function LocalPanel({
  view,
  t,
  err,
  onChange,
  save,
}: PanelProps & { save: (p: Partial<Settings>) => Promise<void> }) {
  const [progress, setProgress] = useState<{ id: string; percent: number } | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    const un = listen<{ id: string; received: number; total: number }>(
      "model-download",
      (e) =>
        setProgress({
          id: e.payload.id,
          percent: Math.floor((e.payload.received / e.payload.total) * 100),
        }),
    );
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const downloading = view.downloading ?? (progress && progress.percent < 100 ? progress.id : null);

  if (!view.localSupported) {
    return <p className="muted">{t("err_CpuUnsupported")}</p>;
  }

  return (
    <div className="stack">
      {view.models.map((m) => {
        const inUse = view.settings.localModel === m.id;
        const isDownloading = downloading === m.id;
        return (
          <div key={m.id} className="row model">
            <div>
              <div>{t(`model_${m.id}` as Parameters<T>[0])}</div>
              <div className="muted small">{formatBytes(m.bytes)}</div>
            </div>
            <div className="actions">
              {m.downloaded ? (
                <>
                  {inUse ? (
                    <span className="badge">{t("inUse")}</span>
                  ) : (
                    <button className="btn ghost" onClick={() => save({ localModel: m.id })}>
                      {t("use")}
                    </button>
                  )}
                  <button
                    className="btn ghost"
                    onClick={async () => {
                      await api.deleteModel(m.id);
                      await onChange();
                    }}
                  >
                    {t("remove")}
                  </button>
                </>
              ) : isDownloading ? (
                <>
                  <span className="muted small">
                    {t("downloading", { percent: progress?.id === m.id ? progress.percent : 0 })}
                  </span>
                  <button className="btn ghost" onClick={() => api.cancelDownload()}>
                    {t("cancel")}
                  </button>
                </>
              ) : (
                <button
                  className="btn"
                  disabled={!!downloading}
                  onClick={async () => {
                    setMessage(null);
                    setProgress({ id: m.id, percent: 0 });
                    const download = api.downloadModel(m.id);
                    await onChange();
                    try {
                      await download;
                      await save({ localModel: m.id });
                    } catch (e) {
                      setMessage(err(errorCode(e)));
                    } finally {
                      setProgress(null);
                      await onChange();
                    }
                  }}
                >
                  {t("download")}
                </button>
              )}
            </div>
          </div>
        );
      })}
      {message ? <p className="warn">{message}</p> : null}
    </div>
  );
}

function OpenRouterPanel({
  view,
  t,
  err,
  onChange,
  save,
}: PanelProps & { save: (p: Partial<Settings>) => Promise<void> }) {
  const [key, setKey] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [models, setModels] = useState<SttModel[] | null>(null);

  useEffect(() => {
    api
      .openRouterModels()
      .then(setModels)
      .catch((e) => setMessage(err(errorCode(e))));
  }, [err]);

  return (
    <div className="stack">
      {view.openrouterKeySet ? (
        <div className="row">
          <span className="muted">{t("openrouterKeySaved")}</span>
          <button
            className="btn ghost"
            onClick={async () => {
              await api.clearOpenRouterKey();
              await onChange();
            }}
          >
            {t("openrouterRemove")}
          </button>
        </div>
      ) : (
        <form
          className="row"
          onSubmit={async (e) => {
            e.preventDefault();
            setBusy(true);
            setMessage(null);
            try {
              await api.setOpenRouterKey(key);
              setKey("");
              await onChange();
            } catch (e) {
              setMessage(err(errorCode(e)));
            } finally {
              setBusy(false);
            }
          }}
        >
          <input
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={key}
            placeholder={t("openrouterKeyPlaceholder")}
            onChange={(e) => setKey(e.target.value)}
          />
          <button className="btn primary" disabled={busy || !key.trim()}>
            {t("openrouterSave")}
          </button>
        </form>
      )}
      {!view.openrouterKeySet ? (
        <button
          className="link"
          type="button"
          onClick={() => openUrl("https://openrouter.ai/settings/keys")}
        >
          {t("openrouterGetKey")}
        </button>
      ) : null}
      <label className="row">
        <span>{t("openrouterModel")}</span>
        <select
          value={view.settings.openrouterModel ?? ""}
          onChange={(e) => save({ openrouterModel: e.target.value || null })}
        >
          <option value="">{t("openrouterChoose")}</option>
          {(models ?? []).map((m) => (
            <option key={m.id} value={m.id}>
              {m.name}
            </option>
          ))}
        </select>
      </label>
      {message ? <p className="warn">{message}</p> : null}
    </div>
  );
}

function ShortcutRow({
  value,
  isMac,
  t,
  onChange,
}: {
  value: string;
  isMac: boolean;
  t: T;
  onChange: (accel: string) => void;
}) {
  const [listening, setListening] = useState(false);

  useEffect(() => {
    if (!listening) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.key === "Escape") {
        setListening(false);
        return;
      }
      const accel = acceleratorFromEvent(e, isMac);
      if (accel) {
        setListening(false);
        if (accel !== value) onChange(accel);
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [listening, isMac, value, onChange]);

  return (
    <div className="stack">
      <div className="row">
        <kbd className={listening ? "listening" : ""}>
          {listening ? t("shortcutPress") : displayShortcut(value, isMac)}
        </kbd>
        <button className="btn ghost" onClick={() => setListening((l) => !l)}>
          {listening ? t("cancel") : t("shortcutChange")}
        </button>
      </div>
      {listening ? <p className="muted small">{t("shortcutHint")}</p> : null}
    </div>
  );
}

function AutostartRow({ t }: { t: T }) {
  const [on, setOn] = useState<boolean | null>(null);
  useEffect(() => {
    isEnabled().then(setOn).catch(() => setOn(false));
  }, []);
  return (
    <label className="row">
      <span>{t("launchAtLogin")}</span>
      <input
        type="checkbox"
        className="switch"
        disabled={on === null}
        checked={!!on}
        onChange={async (e) => {
          const next = e.target.checked;
          try {
            await (next ? enable() : disable());
            setOn(next);
          } catch {
            setOn(await isEnabled());
          }
        }}
      />
    </label>
  );
}

function Footer({ version, t }: { version: string; t: T }) {
  const [update, setUpdate] = useState<Update | null>(null);
  const [status, setStatus] = useState<"idle" | "checking" | "latest" | "installing">("idle");

  const runCheck = useCallback(async (quiet: boolean) => {
    if (!quiet) setStatus("checking");
    try {
      const u = await check();
      setUpdate(u);
      setStatus(u || quiet ? "idle" : "latest");
    } catch {
      setStatus("idle");
    }
  }, []);

  useEffect(() => {
    void runCheck(true);
  }, [runCheck]);

  return (
    <footer className="footer">
      {update ? (
        <div className="card notice">
          <span>{t("updateAvailable", { version: update.version })}</span>
          <button
            className="btn primary"
            disabled={status === "installing"}
            onClick={async () => {
              setStatus("installing");
              await update.downloadAndInstall();
              await relaunch();
            }}
          >
            {t("updateInstall")}
          </button>
        </div>
      ) : null}
      <div className="row muted small">
        <span>
          Zpeak {version} ·{" "}
          <button className="link" onClick={() => openUrl("https://github.com/Zense-AI/zpeak")}>
            {t("openSource")}
          </button>
        </span>
        <button className="link" onClick={() => runCheck(false)} disabled={status === "checking"}>
          {status === "checking"
            ? t("updateChecking")
            : status === "latest"
              ? t("updateLatest")
              : t("updateCheck")}
        </button>
      </div>
    </footer>
  );
}

function formatBytes(bytes: number) {
  return bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : `${Math.round(bytes / 1e6)} MB`;
}
