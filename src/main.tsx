import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";
import { Overlay } from "./Overlay";
import { PermitApp } from "./Permit";
import { SettingsApp } from "./Settings";

const view =
  window.location.hash === "#overlay" ? "overlay" : window.location.hash === "#permit" ? "permit" : "settings";
document.documentElement.dataset.view = view;
document.documentElement.dataset.os = navigator.userAgent.includes("Windows")
  ? "windows"
  : navigator.userAgent.includes("Mac")
    ? "macos"
    : "linux";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {view === "overlay" ? <Overlay /> : view === "permit" ? <PermitApp /> : <SettingsApp />}
  </React.StrictMode>,
);
