// Curation actions shared by the grid, inspector, viewer, menus and keyboard shortcuts.
import { api, errorText } from "./api";
import { bump, bumpStructure, getState } from "./store";
import { toast } from "./components/ui";
import type { CurationUpdate } from "./types";

export async function curate(ids: number[], update: CurationUpdate, quiet = true) {
  if (!ids.length) return;
  try {
    await api.updateCuration(ids, update);
    bump();
    if (!quiet) toast(`Updated ${ids.length} item${ids.length === 1 ? "" : "s"}`, "success");
  } catch (e) {
    toast(errorText(e), "error");
  }
}

export async function addToCollection(collectionId: number, ids: number[]) {
  try {
    const n = await api.addToCollection(collectionId, ids);
    bumpStructure();
    toast(n ? `Added ${n} item${n === 1 ? "" : "s"} to the collection` : "Already in this collection", "success");
  } catch (e) {
    toast(errorText(e), "error");
  }
}

export async function assignTag(tagId: number, ids: number[]) {
  try {
    await api.assignTag(ids, tagId);
    bumpStructure();
  } catch (e) {
    toast(errorText(e), "error");
  }
}

/** Ids the user is acting on: the selection, or the focused item. */
export function targetIds(fallbackId?: number): number[] {
  const s = getState();
  if (fallbackId != null && !s.selection.has(fallbackId)) return [fallbackId];
  if (s.selection.size) return [...s.selection];
  return fallbackId != null ? [fallbackId] : [];
}

import { setState } from "./store";

/** Open the batch-rename or organize dialog for the current selection (or `ids`). */
export function openFileDialog(kind: "rename" | "organize", ids?: number[]) {
  const target = ids ?? targetIds();
  if (!target.length) {
    toast("Select one or more items first", "info");
    return;
  }
  setState({ dialog: { kind, ids: target } });
}
