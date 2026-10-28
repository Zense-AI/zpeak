import { invoke } from "@tauri-apps/api/core";

export type Provider = "zense" | "local" | "openrouter";
export type RecordMode = "toggle" | "hold";

export type Settings = {
  provider: Provider | null;
  shortcut: string;
  mode: RecordMode;
  language: string;
  uiLanguage: string | null;
  localModel: string;
  openrouterModel: string | null;
  restoreClipboard: boolean;
};

export type ModelStatus = { id: string; bytes: number; downloaded: boolean };

export type View = {
  settings: Settings;
  platform: "macos" | "windows" | "linux";
  version: string;
  zenseSignedIn: boolean;
  openrouterKeySet: boolean;
  models: ModelStatus[];
  downloading: string | null;
  accessibility: boolean;
};

export type Profile = { username: string; email: string; eligible: boolean };
export type SttModel = { id: string; name: string };

export type Hud = {
  state: "idle" | "recording" | "transcribing" | "copied" | "error";
  code: string | null;
  level: number;
  seconds: number;
  lang: string | null;
};

/** Rust commands reject with a plain error code string. */
export const errorCode = (e: unknown): string =>
  typeof e === "string" ? e : "Unknown";

export const api = {
  state: () => invoke<View>("get_state"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  zenseSignIn: () => invoke<Profile>("zense_sign_in"),
  zenseProfile: () => invoke<Profile>("zense_profile"),
  zenseSignOut: () => invoke<void>("zense_sign_out"),
  setOpenRouterKey: (key: string) => invoke<void>("set_openrouter_key", { key }),
  clearOpenRouterKey: () => invoke<void>("clear_openrouter_key"),
  openRouterModels: () => invoke<SttModel[]>("openrouter_models"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),
  cancelDownload: () => invoke<void>("cancel_download"),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),
  requestAccessibility: () => invoke<boolean>("request_accessibility"),
  toggleRecording: () => invoke<void>("toggle_recording"),
};
