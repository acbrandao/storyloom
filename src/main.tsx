import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import { setState } from "./store";

if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  const { installDevMock } = await import("./devMock");
  installDevMock();
  const v = new URLSearchParams(location.search).get("view");
  if (v === "grid") setState({ view: { kind: "library", preset: "all" }, selection: new Set([2]), focusIndex: 1 });
}

// Block the webview's own context menu; the app provides its own.
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (!(t.tagName === "INPUT" || t.tagName === "TEXTAREA")) e.preventDefault();
});

// Development aid: VITE_START_VIEW=library:photos | viewer:videos opens a view (or the viewer
// on the first item of a view) at launch — used for screenshots without UI automation.
const startView = import.meta.env.DEV ? (import.meta.env.VITE_START_VIEW as string | undefined) : undefined;
if (startView) {
  const [mode, preset] = startView.split(":");
  setState({ view: { kind: "library", preset: (preset || "all") as any } });
  if (mode === "rename" || mode === "organize") {
    const { api } = await import("./api");
    const dest = import.meta.env.VITE_ORGANIZE_DEST as string | undefined;
    if (dest) localStorage.setItem("mc-organize-dest", dest);
    setTimeout(async () => {
      const ids = (await api.mediaIds({ mediaTypes: ["photo"] }, "captureDate", false)).slice(0, 12);
      setState({ selection: new Set(ids), dialog: { kind: mode, ids } });
    }, 1200);
  } else if (mode === "viewer" || mode === "clips") {
    const { mediaList } = await import("./hooks/mediaList");
    const wait = (): Promise<void> => (mediaList.loaded ? Promise.resolve() : new Promise((r) => setTimeout(r, 100)).then(wait));
    wait().then(() => {
      const first = mediaList.get(0);
      if (first) setState({ viewerId: first.id, selection: new Set([first.id]), focusIndex: 0, clipMode: mode === "clips" });
    });
  } else {
    setTimeout(() => {
      import("./hooks/mediaList").then(({ mediaList }) => {
        const it = mediaList.get(3);
        if (it) setState({ selection: new Set([it.id]), focusIndex: 3, anchorIndex: 3 });
      });
    }, 1500);
  }
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
