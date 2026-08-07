import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import { MainWindow } from "./windows/MainWindow";
import { OverlayLockWindow } from "./windows/OverlayLockWindow";
import { FloatingClockWindow } from "./windows/FloatingClockWindow";
import { SetupWindow } from "./windows/SetupWindow";
import { playSound } from "./lib/sounds";
import { listen } from "@tauri-apps/api/event";
import { EVT } from "./types/tauri-ipc";

// 全局音效监听（各窗口组件也会听；此处兜底主窗未挂载时）
void listen<string>(EVT.playSound, (e) => playSound(e.payload)).catch(() => {});

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
