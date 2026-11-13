import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { FolderPlus, SearchX, X } from "lucide-react";
import { api } from "./api";
import { bump, effectiveFilter, getState, setState, useStore } from "./store";
import { mediaList, useMediaList, useMediaQuerySync } from "./hooks/mediaList";
import type { Collection, CoreEvent, LibraryStats, Source, Tag, View, ColorLabel } from "./types";
import { Sidebar } from "./components/Sidebar";
import { Toolbar, FilterBar } from "./components/Toolbar";
import { MediaGrid } from "./components/MediaGrid";
import { Inspector } from "./components/Inspector";
import { Viewer } from "./components/Viewer";
import { Dashboard } from "./components/Dashboard";
import { ImportDialog } from "./components/ImportDialog";
import { DuplicatesView } from "./components/Duplicates";
import { SettingsView } from "./components/Settings";
import { ActivityView } from "./components/Jobs";
import { ContextMenuHost, Toasts } from "./components/ui";
import { curate, openFileDialog } from "./actions";
import { FileOpsDialogHost } from "./components/FileOpsDialogs";

const PRESET_TITLES: Record<string, string> = {
  all: "All Media", photos: "Photos", videos: "Videos", audio: "Audio", unrated: "Unrated", favorites: "Favorites",
  picks: "Kept", rejected: "Rejected", review: "Needs Review", recent: "Recently Added", missing: "Missing Files", nodate: "No Capture Date",
};

function viewTitle(v: View, collections: Collection[], tags: Tag[]): string {
  switch (v.kind) {
    case "dashboard": return "Dashboard";
    case "library": return PRESET_TITLES[v.preset];
    case "collection": return collections.find((c) => c.id === v.id)?.name ?? "Collection";
    case "tag": return tags.find((t) => t.id === v.id)?.path ?? "Tag";
    case "folder": return v.path.split(/[\\/]/).pop() || v.path;
    case "duplicates": return "Duplicate Finder";
    case "activity": return "Activity & History";
    case "settings": return "Settings";
  }
}

const COLOR_KEYS: Record<string, ColorLabel> = { "6": "red", "7": "yellow", "8": "green", "9": "blue" };

export default function App() {
  const view = useStore((s) => s.view);
  const filter = useStore((s) => s.filter);
  const sort = useStore((s) => s.sort);
  const descending = useStore((s) => s.descending);
  const revision = useStore((s) => s.revision);
  const structureRevision = useStore((s) => s.structureRevision);
  const leftOpen = useStore((s) => s.leftOpen);
  const rightOpen = useStore((s) => s.rightOpen);
  const showFilters = useStore((s) => s.showFilters);
  const viewerId = useStore((s) => s.viewerId);
  const theme = useStore((s) => s.theme);
  const autoAdvance = useStore((s) => s.autoAdvance);
  const selection = useStore((s) => s.selection);

  const [stats, setStats] = useState<LibraryStats | null>(null);
  const [sources, setSources] = useState<Source[]>([]);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [tags, setTags] = useState<Tag[]>([]);
  const [cameras, setCameras] = useState<string[]>([]);
  const [importOpen, setImportOpen] = useState<{ path?: string } | null>(null);
  const [dropping, setDropping] = useState(false);

  const isGrid = !["dashboard", "duplicates", "activity", "settings"].includes(view.kind);
  const gridFilter = useMemo(() => (isGrid ? effectiveFilter(view, filter) : null), [isGrid, view, filter]);
  useMediaQuerySync(gridFilter, sort, descending, revision);
  const list = useMediaList();

  // Theme
  useEffect(() => {
    const apply = () => {
      const dark = theme === "dark" || (theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
      document.documentElement.dataset.theme = dark ? "dark" : "light";
    };
    apply();
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [theme]);
  useEffect(() => { (window as any).__autoAdvance = autoAdvance; }, [autoAdvance]);

  // Sidebar data
  useEffect(() => {
    api.sources().then(setSources).catch(() => {});
    api.collections().then(setCollections).catch(() => {});
    api.tags().then(setTags).catch(() => {});
  }, [structureRevision]);
  const statsTimer = useRef<number | null>(null);
  useEffect(() => {
    if (statsTimer.current) return;
    statsTimer.current = window.setTimeout(() => {
      statsTimer.current = null;
      api.stats().then(setStats).catch(() => {});
      api.collections().then(setCollections).catch(() => {});
      api.cameras().then(setCameras).catch(() => {});
    }, 250);
  }, [revision]);

  // Core events: job progress and catalog changes.
  useEffect(() => {
    let lastBump = 0;
    let pending: number | null = null;
    const un = listen<CoreEvent>("core-event", (e) => {
      const ev = e.payload;
      if (ev.kind === "jobProgress") {
        api.activeJobs().then((jobs) => setState({ jobs })).catch(() => {});
        if (ev.status !== "running") {
          api.sources().then(setSources).catch(() => {});
          setState((s) => ({ structureRevision: s.structureRevision + 1 }));
        }
      } else {
        const now = Date.now();
        if (now - lastBump > 900) { lastBump = now; bump(); }
        else if (!pending) pending = window.setTimeout(() => { pending = null; lastBump = Date.now(); bump(); }, 900);
      }
    });
    api.activeJobs().then((jobs) => setState({ jobs })).catch(() => {});
    return () => { un.then((f) => f()); };
  }, []);

  // Drag folders from Finder onto the window.
  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "over" || p.type === "enter") setDropping(true);
      else if (p.type === "leave") setDropping(false);
      else if (p.type === "drop") {
        setDropping(false);
        if (p.paths?.length) setImportOpen({ path: p.paths[0] });
      }
    });
    return () => { un.then((f) => f()); };
  }, []);

  // Grid keyboard shortcuts.
  const onKey = useCallback(async (e: KeyboardEvent) => {
    const s = getState();
    const t = e.target as HTMLElement;
    if (s.viewerId != null || t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || document.querySelector(".modal-backdrop") || s.dialog) return;
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "i") { e.preventDefault(); setState({ rightOpen: !s.rightOpen }); return; }
    if (!isGrid) return;
    const cols = (window as any).__gridCols ?? 1;
    const total = mediaList.total;
    const focusIdx = s.focusIndex >= 0 ? s.focusIndex : -1;
    const ids = s.selection.size ? [...s.selection] : (() => { const it = mediaList.get(focusIdx); return it ? [it.id] : []; })();
    const moveTo = async (i: number, extend: boolean) => {
      i = Math.max(0, Math.min(total - 1, i));
      const it = await mediaList.getAsync(i);
      if (!it) return;
      if (extend && s.anchorIndex >= 0) {
        const sel = new Set<number>();
        const [a, b] = [Math.min(s.anchorIndex, i), Math.max(s.anchorIndex, i)];
        for (let k = a; k <= b; k++) { const x = mediaList.get(k); if (x) sel.add(x.id); }
        setState({ selection: sel, focusIndex: i });
      } else setState({ selection: new Set([it.id]), focusIndex: i, anchorIndex: i });
    };
    const advance = () => { if (s.autoAdvance && ids.length === 1) moveTo(focusIdx + 1, false); };
    const k = e.key;
    const lower = k.toLowerCase();
    let handled = true;
    if (k === "ArrowRight") moveTo(focusIdx + 1, e.shiftKey);
    else if (k === "ArrowLeft") moveTo(focusIdx - 1, e.shiftKey);
    else if (k === "ArrowDown") moveTo(focusIdx < 0 ? 0 : focusIdx + cols, e.shiftKey);
    else if (k === "ArrowUp") moveTo(focusIdx - cols, e.shiftKey);
    else if (k === "Home") moveTo(0, e.shiftKey);
    else if (k === "End") moveTo(total - 1, e.shiftKey);
    else if ((e.metaKey || e.ctrlKey) && lower === "a") {
      const all = await api.mediaIds(gridFilter ?? {}, sort, descending);
      setState({ selection: new Set(all) });
    } else if (k === "Escape") setState({ selection: new Set(), focusIndex: -1 });
    else if (k === "F2" && ids.length) openFileDialog("rename", ids);
    else if ((k === " " || k === "Enter") && ids.length) setState({ viewerId: mediaList.get(focusIdx)?.id ?? ids[0] });
    else if (e.metaKey || e.ctrlKey || e.altKey) handled = false;
    else if (/^[0-5]$/.test(k) && ids.length) { await curate(ids, { rating: Number(k) }); advance(); }
    else if (COLOR_KEYS[k] && ids.length) { const cur = mediaList.get(focusIdx)?.colorLabel; curate(ids, { colorLabel: cur === COLOR_KEYS[k] && ids.length === 1 ? "" : COLOR_KEYS[k] }); }
    else if (lower === "p" && ids.length) { await curate(ids, { selectionStatus: "selected" }); advance(); }
    else if (lower === "x" && ids.length) { await curate(ids, { selectionStatus: "rejected" }); advance(); }
    else if (lower === "u" && ids.length) curate(ids, { selectionStatus: "none" });
    else if (lower === "f" && ids.length) curate(ids, { favorite: !mediaList.get(focusIdx)?.favorite });
    else if (lower === "r" && ids.length) curate(ids, { reviewStatus: mediaList.get(focusIdx)?.reviewStatus === "needs_review" ? "none" : "needs_review" });
    else if (k === "+" || k === "=") setState({ thumbSize: Math.min(420, s.thumbSize + 30) });
    else if (k === "-") setState({ thumbSize: Math.max(110, s.thumbSize - 30) });
    else handled = false;
    if (handled) e.preventDefault();
  }, [isGrid, gridFilter, sort, descending]);
  useEffect(() => {
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onKey]);

  const openImport = () => setImportOpen({});
  const title = viewTitle(view, collections, tags);
  const hasUserFilter = Object.keys(filter).some((k) => (filter as any)[k] != null && (filter as any)[k] !== "" && (filter as any)[k] !== false);

  const emptyState = hasUserFilter ? (
    <div className="empty-state"><SearchX size={44} /><h2>Nothing matches</h2><p>Try removing some filters or search words.</p>
      <button className="btn" onClick={() => setState({ filter: {} })}>Clear filters</button></div>
  ) : stats?.totalMedia === 0 ? (
    <div className="empty-state"><FolderPlus size={44} /><h2>No media yet</h2><p>Add a folder to start reviewing your photos, videos and audio.</p>
      <button className="btn primary" onClick={openImport}>Add a Folder…</button></div>
  ) : (
    <div className="empty-state"><h2>Nothing here yet</h2>
      <p>{view.kind === "collection" ? "Drag media onto this collection in the sidebar, or use “Add to collection” in the details panel." : view.kind === "tag" ? "Drag media onto this tag in the sidebar, or add the tag from the details panel." : "No media in this view."}</p></div>
  );

  return (
    <div className="app">
      {leftOpen && <Sidebar stats={stats} sources={sources} collections={collections} tags={tags} onImport={openImport} />}
      <div className="main">
        <Toolbar title={title} count={isGrid && list.loaded ? list.total : undefined} onImport={openImport} isGrid={isGrid} />
        {isGrid && showFilters && <FilterBar tags={tags} cameras={cameras} />}
        {isGrid && selection.size > 1 && (
          <div className="selection-bar">
            <b>{selection.size.toLocaleString()} selected</b>
            <span className="subtle">Rate with 0–5, P / X to keep or reject, or drag onto a collection or tag.</span>
            <span className="spacer" />
            <button className="btn small ghost" onClick={() => setState({ selection: new Set() })}><X size={12} />Clear selection</button>
          </div>
        )}
        <div className="content">
          <div className="center">
            {view.kind === "dashboard" && <Dashboard stats={stats} sources={sources} onImport={openImport} />}
            {view.kind === "duplicates" && <DuplicatesView />}
            {view.kind === "activity" && <ActivityView />}
            {view.kind === "settings" && <SettingsView />}
            {isGrid && (list.error ? <div className="empty-state"><p className="error-text">{list.error}</p></div> : <MediaGrid collections={collections} emptyState={emptyState} />)}
          </div>
          {isGrid && rightOpen && <Inspector tags={tags} collections={collections} />}
        </div>
      </div>
      {viewerId != null && <Viewer />}
      {importOpen && <ImportDialog initialPath={importOpen.path} onClose={() => setImportOpen(null)} />}
      {dropping && <div className="drop-overlay">Drop a folder to add it to your library</div>}
      <FileOpsDialogHost />
      <ContextMenuHost />
      <Toasts />
    </div>
  );
}
