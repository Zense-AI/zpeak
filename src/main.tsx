import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";
import { Overlay } from "./Overlay";
import { SettingsApp } from "./Settings";

const isOverlay = window.location.hash === "#overlay";
document.documentElement.dataset.view = isOverlay ? "overlay" : "settings";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isOverlay ? <Overlay /> : <SettingsApp />}</React.StrictMode>,
);
