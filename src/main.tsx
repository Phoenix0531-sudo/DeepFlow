import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import { MainWindow } from "./windows/MainWindow";
import { OverlayLockWindow } from "./windows/OverlayLockWindow";
import { FloatingClockWindow } from "./windows/FloatingClockWindow";
import { SetupWindow } from "./windows/SetupWindow";

function resolveWindow(): "main" | "overlay" | "floating" | "setup" {
  const q = new URLSearchParams(window.location.search).get("window");
  if (q === "overlay" || q === "floating" || q === "setup") return q;
  return "main";
}

const map = {
  main: <MainWindow />,
  overlay: <OverlayLockWindow />,
  floating: <FloatingClockWindow />,
  setup: <SetupWindow />,
} as const;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{map[resolveWindow()]}</React.StrictMode>,
);
