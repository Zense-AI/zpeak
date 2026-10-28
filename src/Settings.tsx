import { useCallback, useEffect, useMemo, useState } from "react";
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
import logo from "./logo.svg";

const LANGUAGES = ["auto", "en", "zh", "yue", "ja", "ko", "fr", "de", "es"] as const;

export function SettingsApp() {
  const [view, setView] = useState<View | null>(null);
  const [error, setError] = useState<string | null>(null);

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

  return (
    <main className="page">
      <header className="hero">
        <img src={logo} alt="" className="hero-logo" />
        <div>
          <h1>Zpeak</h1>
          <p className="muted">
            {t("tagline", { shortcut: displayShortcut(view.settings.shortcut, isMac) })}
          </p>
        </div>
      </header>

      {error ? (
        <p className="alert" role="alert">
          {error}
        </p>
      ) : null}

      <Section title={t("providerTitle")}>
        <ProviderCard
          id="zense"
          active={view.settings.provider}
          onSelect={(provider) => save({ provider })}
          title={t("zenseTitle")}
          badge={t("zenseBadge")}
          desc={t("zenseDesc")}
          t={t}
        >
          <ZensePanel view={view} t={t} err={err} onChange={refresh} />
        </ProviderCard>
        <ProviderCard
          id="local"
          active={view.settings.provider}
          onSelect={(provider) => save({ provider })}
          title={t("localTitle")}
          desc={t("localDesc")}
          t={t}
        >
          <LocalPanel view={view} t={t} err={err} onChange={refresh} save={save} />
        </ProviderCard>
        <ProviderCard
          id="openrouter"
          active={view.settings.provider}
          onSelect={(provider) => save({ provider })}
          title={t("openrouterTitle")}
          desc={t("openrouterDesc")}
          t={t}
        >
          <OpenRouterPanel view={view} t={t} err={err} onChange={refresh} save={save} />
        </ProviderCard>
      </Section>

      <Section title={t("shortcutTitle")}>
        <div className="card">
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
                role="radio"
                aria-checked={view.settings.mode === mode}
                className={view.settings.mode === mode ? "on" : ""}
                onClick={() => save({ mode })}
              >
                {mode === "toggle" ? t("modeToggle") : t("modeHold")}
              </button>
            ))}
          </div>
        </div>
      </Section>

      <Section title={t("languageTitle")}>
        <div className="card rows">
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
        </div>
      </Section>

      <Section title={t("tryTitle")}>
        <textarea
          className="try"
          rows={3}
          placeholder={t("tryPlaceholder", {
            shortcut: displayShortcut(view.settings.shortcut, isMac),
          })}
        />
      </Section>

      <Section title={t("generalTitle")}>
        {isMac && !view.accessibility ? (
          <div className="card notice">
            <strong>{t("accessibilityTitle")}</strong>
            <p className="muted">{t("accessibilityBody")}</p>
            <button
              className="btn"
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
          </div>
        ) : null}
        <div className="card rows">
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
      </Section>

      <Footer version={view.version} t={t} />
    </main>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="section">
      <h2>{title}</h2>
      {children}
    </section>
  );
}

function ProviderCard(props: {
  id: Provider;
  active: Provider | null;
  onSelect: (p: Provider) => void;
  title: string;
  badge?: string;
  desc: string;
  t: T;
  children: React.ReactNode;
}) {
  const selected = props.active === props.id;
  return (
    <div className={`card provider ${selected ? "selected" : ""}`}>
      <label className="provider-head">
        <input
          type="radio"
          name="provider"
          checked={selected}
          onChange={() => props.onSelect(props.id)}
        />
        <div>
          <div className="provider-title">
            {props.title}
            {props.badge ? <span className="badge">{props.badge}</span> : null}
          </div>
          <p className="muted">{props.desc}</p>
        </div>
      </label>
      {selected ? <div className="provider-body">{props.children}</div> : null}
    </div>
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

  if (view.zenseSignedIn && profile) {
    return (
      <div className="stack">
        <div className="row">
          <span>{t("zenseSignedInAs", { email: profile.email })}</span>
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
        {!profile.eligible ? <p className="warn">{t("zenseIneligible")}</p> : null}
      </div>
    );
  }

  return (
    <div className="stack">
      <button
        className="btn primary"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          setMessage(null);
          try {
            const p = await api.zenseSignIn();
            setProfile(p);
            await onChange();
          } catch (e) {
            setMessage(err(errorCode(e)));
          } finally {
            setBusy(false);
          }
        }}
      >
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
