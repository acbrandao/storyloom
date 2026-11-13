import { useEffect, useMemo, useState } from "react";
import {
  AlertTriangle, Ban, CalendarX, ChevronDown, ChevronRight, Clock, Copy, Film, Folder, FolderOpen, HardDrive, Heart, History,
  Image, Images, LayoutDashboard, PenLine, FolderInput, Library, ListChecks, Music, Plus, RefreshCw, Settings, Sparkles, Star, Tag as TagIcon, Trash2, FolderSearch, Pencil,
} from "lucide-react";
import { api, errorText } from "../api";
import { bumpStructure, navigate, useStore } from "../store";
import type { Collection, LibraryPreset, Source, Tag, View, LibraryStats } from "../types";
import { confirmDialog, openMenu, promptText, toast } from "./ui";
import { addToCollection, assignTag, openFileDialog } from "../actions";
import { JobsPanel } from "./Jobs";

function sameView(a: View, b: View) {
  return JSON.stringify(a) === JSON.stringify(b);
}

function NavItem({ view, icon, label, count, depth = 0, onContextMenu, dropHandler, twisty, className }: {
  view: View; icon: React.ReactNode; label: string; count?: number | string; depth?: number;
  onContextMenu?: (e: React.MouseEvent) => void; dropHandler?: (ids: number[]) => void; twisty?: React.ReactNode; className?: string;
}) {
  const current = useStore((s) => s.view);
  const [over, setOver] = useState(false);
  return (
    <div
      className={`nav-item ${sameView(current, view) ? "active" : ""} ${over ? "drop-target" : ""} ${className ?? ""}`}
      style={{ paddingLeft: 8 + depth * 14 }}
      onClick={() => navigate(view)}
      onContextMenu={onContextMenu}
      onDragOver={dropHandler ? (e) => { if (e.dataTransfer.types.includes("application/x-media-ids")) { e.preventDefault(); setOver(true); } } : undefined}
      onDragLeave={() => setOver(false)}
      onDrop={dropHandler ? (e) => {
        setOver(false);
        const raw = e.dataTransfer.getData("application/x-media-ids");
        if (raw) { e.preventDefault(); dropHandler(JSON.parse(raw)); }
      } : undefined}
    >
      {twisty}
      {icon}
      <span className="label">{label}</span>
      {count != null && <span className="count">{typeof count === "number" ? count.toLocaleString() : count}</span>}
    </div>
  );
}

export function Sidebar({ stats, sources, collections, tags, onImport }: {
  stats: LibraryStats | null; sources: Source[]; collections: Collection[]; tags: Tag[]; onImport: () => void;
}) {
  const lib = (preset: LibraryPreset): View => ({ kind: "library", preset });
  return (
    <aside className="sidebar">
      <div className="sidebar-top drag" data-tauri-drag-region>
        <span className="brand" data-tauri-drag-region>STORYLOOM</span>
      </div>
      <div className="sidebar-scroll">
        <NavItem view={{ kind: "dashboard" }} icon={<LayoutDashboard size={16} />} label="Dashboard" />

        <div className="nav-section">
          <div className="nav-title">Library</div>
          <NavItem view={lib("all")} icon={<Library size={16} />} label="All Media" count={stats?.totalMedia} />
          <NavItem view={lib("photos")} icon={<Image size={16} />} label="Photos" count={stats?.totalPhotos} />
          <NavItem view={lib("videos")} icon={<Film size={16} />} label="Videos" count={stats?.totalVideos} />
          <NavItem view={lib("audio")} icon={<Music size={16} />} label="Audio" count={stats?.totalAudio} />
          <NavItem view={lib("unrated")} icon={<Star size={16} />} label="Unrated" count={stats?.totalUnrated} />
          <NavItem view={lib("picks")} icon={<ListChecks size={16} />} label="Kept" count={stats?.totalSelected} />
          <NavItem view={lib("favorites")} icon={<Heart size={16} />} label="Favorites" count={stats?.totalFavorites} />
          <NavItem view={lib("review")} icon={<Clock size={16} />} label="Needs Review" count={stats?.totalNeedsReview} />
          <NavItem view={lib("rejected")} icon={<Ban size={16} />} label="Rejected" count={stats?.totalRejected} />
          <NavItem view={lib("recent")} icon={<Sparkles size={16} />} label="Recently Added" />
          {!!stats?.withoutCaptureDate && <NavItem view={lib("nodate")} icon={<CalendarX size={16} />} label="No Capture Date" count={stats.withoutCaptureDate} />}
          {!!stats?.missing && <NavItem view={lib("missing")} icon={<AlertTriangle size={16} />} label="Missing Files" count={stats.missing} />}
        </div>

        <FoldersSection sources={sources} onImport={onImport} />
        <CollectionsSection collections={collections} />
        <TagsSection tags={tags} />

        <div className="nav-section">
          <div className="nav-title">Tools</div>
          <NavItem view={{ kind: "duplicates" }} icon={<Copy size={16} />} label="Duplicate Finder" count={stats?.duplicateGroups || undefined} />
          <div className="nav-item" onClick={() => openFileDialog("rename")} title="Rename the selected files (F2)"><PenLine size={16} /><span className="label">Batch Rename…</span></div>
          <div className="nav-item" onClick={() => openFileDialog("organize")} title="Copy or move the selected files into folders"><FolderInput size={16} /><span className="label">Organize into Folders…</span></div>
          <NavItem view={{ kind: "activity" }} icon={<History size={16} />} label="Activity & History" />
          <NavItem view={{ kind: "settings" }} icon={<Settings size={16} />} label="Settings" />
        </div>
      </div>
      <div className="sidebar-footer">
        <JobsPanel compact />
      </div>
    </aside>
  );
}

// ---------------------------------------------------------------- folders

interface FolderNode { name: string; path: string; count: number; total: number; children: FolderNode[] }

function buildTree(root: string, folders: [string, number][]): FolderNode {
  const node: FolderNode = { name: "", path: root, count: 0, total: 0, children: [] };
  const sep = root.includes("\\") ? "\\" : "/";
  for (const [path, count] of folders) {
    if (path !== root && !path.startsWith(root + sep)) continue;
    const rel = path.slice(root.length).split(sep).filter(Boolean);
    let cur = node;
    let curPath = root;
    cur.total += count;
    for (const part of rel) {
      curPath = curPath + sep + part;
      let child = cur.children.find((c) => c.name === part);
      if (!child) {
        child = { name: part, path: curPath, count: 0, total: 0, children: [] };
        cur.children.push(child);
      }
      child.total += count;
      cur = child;
    }
    cur.count += count;
  }
  const sort = (n: FolderNode) => { n.children.sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true })); n.children.forEach(sort); };
  sort(node);
  return node;
}

function FoldersSection({ sources, onImport }: { sources: Source[]; onImport: () => void }) {
  const structureRevision = useStore((s) => s.structureRevision);
  const revision = useStore((s) => s.revision);
  const [folders, setFolders] = useState<[string, number][]>([]);
  const [open, setOpen] = useState<Set<string>>(new Set());
  useEffect(() => {
    api.folders().then(setFolders).catch(() => {});
  }, [structureRevision, Math.floor(revision / 5), sources.length]);
  const trees = useMemo(() => sources.map((s) => ({ source: s, tree: buildTree(s.path, folders) })), [sources, folders]);

  const toggle = (p: string) => setOpen((o) => { const n = new Set(o); n.has(p) ? n.delete(p) : n.add(p); return n; });

  const sourceMenu = (e: React.MouseEvent, s: Source) =>
    openMenu(e, [
      { label: "Rescan for changes", icon: <RefreshCw size={14} />, onClick: async () => { try { await api.rescanSource(s.id); } catch (err) { toast(errorText(err), "error"); } } },
      { label: "Show in Finder", icon: <FolderSearch size={14} />, onClick: () => api.revealPath(s.path) },
      { separator: true },
      {
        label: "Remove from catalog…", icon: <Trash2 size={14} />, danger: true,
        onClick: async () => {
          const ok = await confirmDialog({
            title: `Remove “${s.name}” from the catalog?`,
            message: <>Ratings, tags and collection membership for its {s.mediaCount.toLocaleString()} items will be removed from StoryLoom. <b>No files will be deleted or changed on disk.</b></>,
            okLabel: "Remove from Catalog", danger: true,
          });
          if (!ok) return;
          try { await api.removeSource(s.id); bumpStructure(); navigate({ kind: "dashboard" }); toast("Folder removed from the catalog", "success"); } catch (err) { toast(errorText(err), "error"); }
        },
      },
    ]);

  const renderNode = (n: FolderNode, s: Source, depth: number): React.ReactNode => {
    const isOpen = open.has(n.path);
    return (
      <div key={n.path}>
        <NavItem
          view={{ kind: "folder", path: n.path, sourceId: s.id }}
          depth={depth}
          icon={isOpen ? <FolderOpen size={15} /> : <Folder size={15} />}
          label={n.name}
          count={n.total}
          twisty={<span className="twisty" onClick={(e) => { e.stopPropagation(); toggle(n.path); }}>{n.children.length ? (isOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />) : null}</span>}
          onContextMenu={(e) => openMenu(e, [{ label: "Show in Finder", icon: <FolderSearch size={14} />, onClick: () => api.revealPath(n.path) }])}
        />
        {isOpen && n.children.map((c) => renderNode(c, s, depth + 1))}
      </div>
    );
  };

  return (
    <div className="nav-section">
      <div className="nav-title">
        Folders
        <button className="icon-btn" style={{ width: 20, height: 20 }} title="Add folder or import" onClick={onImport}><Plus size={14} /></button>
      </div>
      {sources.length === 0 && <div className="nav-item subtle" onClick={onImport}><Plus size={15} /> <span className="label">Add a folder…</span></div>}
      {trees.map(({ source: s, tree }) => {
        const isOpen = open.has(s.path);
        return (
          <div key={s.id}>
            <NavItem
              view={{ kind: "folder", path: s.path, sourceId: s.id }}
              icon={<HardDrive size={15} />}
              label={s.name}
              count={s.isAvailable ? s.mediaCount : "offline"}
              className={s.isAvailable ? "" : "unavailable"}
              twisty={<span className="twisty" onClick={(e) => { e.stopPropagation(); toggle(s.path); }}>{tree.children.length ? (isOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />) : null}</span>}
              onContextMenu={(e) => sourceMenu(e, s)}
            />
            {isOpen && tree.children.map((c) => renderNode(c, s, 1))}
          </div>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------- collections

function CollectionsSection({ collections }: { collections: Collection[] }) {
  const create = async () => {
    const name = await promptText({ title: "New Collection", label: "Name", placeholder: "e.g. Brazil 2026 Highlights", okLabel: "Create",
      hint: "Collections group media virtually — files are never moved or copied." });
    if (!name) return;
    try { const id = await api.createCollection(name); bumpStructure(); navigate({ kind: "collection", id }); } catch (e) { toast(errorText(e), "error"); }
  };
  const menu = (e: React.MouseEvent, c: Collection) =>
    openMenu(e, [
      { label: "Rename…", icon: <Pencil size={14} />, onClick: async () => {
        const name = await promptText({ title: "Rename Collection", initial: c.name, okLabel: "Rename" });
        if (name) { await api.renameCollection(c.id, name).catch((x) => toast(errorText(x), "error")); bumpStructure(); }
      } },
      { separator: true },
      { label: "Delete Collection…", icon: <Trash2 size={14} />, danger: true, onClick: async () => {
        if (await confirmDialog({ title: `Delete “${c.name}”?`, message: "Only the collection is deleted. The media in it stay in your library and on disk.", okLabel: "Delete", danger: true })) {
          await api.deleteCollection(c.id); bumpStructure(); navigate({ kind: "library", preset: "all" });
        }
      } },
    ]);
  const manual = collections.filter((c) => c.collectionType === "manual");
  const smart = collections.filter((c) => c.collectionType === "smart");
  return (
    <div className="nav-section">
      <div className="nav-title">
        Collections
        <button className="icon-btn" style={{ width: 20, height: 20 }} title="New collection" onClick={create}><Plus size={14} /></button>
      </div>
      {collections.length === 0 && <div className="nav-item subtle" onClick={create}><Plus size={15} /><span className="label">New collection…</span></div>}
      {manual.map((c) => (
        <NavItem key={c.id} view={{ kind: "collection", id: c.id }} icon={<Images size={15} />} label={c.name} count={c.mediaCount}
          onContextMenu={(e) => menu(e, c)} dropHandler={(ids) => addToCollection(c.id, ids)} />
      ))}
      {smart.map((c) => (
        <NavItem key={c.id} view={{ kind: "collection", id: c.id }} icon={<Sparkles size={15} />} label={c.name} count={c.mediaCount} onContextMenu={(e) => menu(e, c)} />
      ))}
    </div>
  );
}

// ---------------------------------------------------------------- tags

function TagsSection({ tags }: { tags: Tag[] }) {
  const [open, setOpen] = useState<Set<number>>(new Set());
  const children = useMemo(() => {
    const m = new Map<number | null, Tag[]>();
    tags.forEach((t) => { const k = t.parentId; m.set(k, [...(m.get(k) ?? []), t]); });
    return m;
  }, [tags]);
  const totals = useMemo(() => {
    const memo = new Map<number, number>();
    const total = (t: Tag): number => {
      if (memo.has(t.id)) return memo.get(t.id)!;
      const v = t.mediaCount + (children.get(t.id) ?? []).reduce((a, c) => a + total(c), 0);
      memo.set(t.id, v);
      return v;
    };
    tags.forEach(total);
    return memo;
  }, [tags, children]);

  const create = async (parent: Tag | null) => {
    const name = await promptText({
      title: parent ? `New tag inside “${parent.name}”` : "New Tag",
      label: "Name",
      placeholder: parent ? "e.g. Florianopolis" : "e.g. Locations/Brazil/Rio de Janeiro",
      hint: parent ? undefined : "Use “/” to create a hierarchy in one step.",
      okLabel: "Create",
    });
    if (!name) return;
    try { await api.createTag(name, parent?.id ?? null); if (parent) setOpen((o) => new Set(o).add(parent.id)); bumpStructure(); } catch (e) { toast(errorText(e), "error"); }
  };
  const menu = (e: React.MouseEvent, t: Tag) =>
    openMenu(e, [
      { label: "New tag inside…", icon: <Plus size={14} />, onClick: () => create(t) },
      { label: "Rename…", icon: <Pencil size={14} />, onClick: async () => {
        const name = await promptText({ title: "Rename Tag", initial: t.name, okLabel: "Rename" });
        if (name) { await api.renameTag(t.id, name).catch((x) => toast(errorText(x), "error")); bumpStructure(); }
      } },
      { label: "Move to top level", disabled: t.parentId == null, onClick: async () => { await api.moveTag(t.id, null); bumpStructure(); } },
      { separator: true },
      { label: "Delete Tag…", icon: <Trash2 size={14} />, danger: true, onClick: async () => {
        if (await confirmDialog({ title: `Delete tag “${t.path}”?`, message: "The tag and any tags inside it are removed from all media. Files are not affected.", okLabel: "Delete", danger: true })) {
          await api.deleteTag(t.id); bumpStructure(); navigate({ kind: "library", preset: "all" });
        }
      } },
    ]);
  const render = (t: Tag, depth: number): React.ReactNode => {
    const kids = children.get(t.id) ?? [];
    const isOpen = open.has(t.id);
    return (
      <div key={t.id}>
        <NavItem view={{ kind: "tag", id: t.id }} depth={depth} icon={<TagIcon size={14} />} label={t.name} count={totals.get(t.id)}
          twisty={<span className="twisty" onClick={(e) => { e.stopPropagation(); setOpen((o) => { const n = new Set(o); n.has(t.id) ? n.delete(t.id) : n.add(t.id); return n; }); }}>
            {kids.length ? (isOpen ? <ChevronDown size={12} /> : <ChevronRight size={12} />) : null}</span>}
          onContextMenu={(e) => menu(e, t)} dropHandler={(ids) => assignTag(t.id, ids)} />
        {isOpen && kids.map((k) => render(k, depth + 1))}
      </div>
    );
  };
  return (
    <div className="nav-section">
      <div className="nav-title">
        Tags
        <button className="icon-btn" style={{ width: 20, height: 20 }} title="New tag" onClick={() => create(null)}><Plus size={14} /></button>
      </div>
      {tags.length === 0 && <div className="nav-item subtle" onClick={() => create(null)}><Plus size={15} /><span className="label">New tag…</span></div>}
      {(children.get(null) ?? []).map((t) => render(t, 0))}
    </div>
  );
}
