// Mirrors curator-core models (serde camelCase).
export type MediaType = "photo" | "video" | "audio";
export type SortField = "captureDate" | "filename" | "rating" | "fileSize" | "addedAt" | "duration";
export type Selection = "none" | "selected" | "rejected";
export type Review = "none" | "needs_review" | "reviewed";
export type ColorLabel = "red" | "orange" | "yellow" | "green" | "blue" | "purple";

export interface MediaFilter {
  text?: string | null;
  mediaTypes?: MediaType[];
  minRating?: number | null;
  maxRating?: number | null;
  unratedOnly?: boolean;
  selectionStatus?: Selection | null;
  reviewStatus?: Review | null;
  favoriteOnly?: boolean;
  colorLabel?: ColorLabel | null;
  tagIds?: number[];
  collectionId?: number | null;
  sourceId?: number | null;
  folder?: string | null;
  dateFrom?: string | null;
  dateTo?: string | null;
  camera?: string | null;
  minDuration?: number | null;
  maxDuration?: number | null;
  minWidth?: number | null;
  addedSinceMs?: number | null;
  missingOnly?: boolean;
  noCaptureDate?: boolean;
  hasLocation?: boolean | null;
}

export interface MediaQuery {
  filter: MediaFilter;
  sort: SortField;
  descending: boolean;
  offset: number;
  limit: number;
}

export interface MediaSummary {
  id: number;
  mediaType: MediaType;
  filename: string;
  sortDatetime: string | null;
  captureKnown: boolean;
  width: number | null;
  height: number | null;
  duration: number | null;
  rating: number;
  selectionStatus: Selection;
  reviewStatus: Review;
  favorite: boolean;
  colorLabel: ColorLabel | null;
  thumbnailStatus: string;
  isMissing: boolean;
  fileSize: number;
}

export interface MediaPage {
  total: number;
  offset: number;
  items: MediaSummary[];
}

export interface Tag {
  id: number;
  name: string;
  parentId: number | null;
  path: string;
  mediaCount: number;
}

export interface CollectionRef {
  id: number;
  name: string;
}

export interface Collection {
  id: number;
  name: string;
  description: string | null;
  collectionType: "manual" | "smart";
  rules: MediaFilter | null;
  mediaCount: number;
  createdAt: number;
}

export interface MediaDetail {
  id: number;
  sourceId: number | null;
  originalFilename: string;
  currentFilename: string;
  originalPath: string;
  currentPath: string;
  folderPath: string;
  extension: string;
  mediaType: MediaType;
  mimeType: string | null;
  fileSize: number;
  fileHash: string | null;
  captureDatetime: string | null;
  captureTimezone: string | null;
  captureTimeSource: string | null;
  userCaptureDatetime: string | null;
  userCaptureTimezone: string | null;
  sortDatetime: string | null;
  filesystemCreationTime: number | null;
  filesystemModificationTime: number;
  width: number | null;
  height: number | null;
  orientation: number | null;
  duration: number | null;
  codec: string | null;
  audioCodec: string | null;
  frameRate: number | null;
  bitrate: number | null;
  sampleRate: number | null;
  cameraMake: string | null;
  cameraModel: string | null;
  lens: string | null;
  exposure: string | null;
  gpsLatitude: number | null;
  gpsLongitude: number | null;
  gpsAltitude: number | null;
  title: string | null;
  artist: string | null;
  album: string | null;
  genre: string | null;
  embeddedCaption: string | null;
  embeddedKeywords: string | null;
  rating: number;
  selectionStatus: Selection;
  reviewStatus: Review;
  favorite: boolean;
  colorLabel: ColorLabel | null;
  description: string | null;
  location: string | null;
  notes: string | null;
  metadataStatus: string;
  metadataError: string | null;
  thumbnailStatus: string;
  isMissing: boolean;
  addedAt: number;
  lastScannedAt: number;
  tags: Tag[];
  collections: CollectionRef[];
  derivedSegments: number;
  clipOrigin: ClipOrigin | null;
}

export interface ClipOrigin { segmentId: number; sourceMediaId: number; sourceFilename: string; inPoint: number; outPoint: number; actualInPoint: number | null; processingMethod: string | null; preset: string | null }

export interface CurationUpdate {
  rating?: number;
  selectionStatus?: Selection;
  reviewStatus?: Review;
  favorite?: boolean;
  colorLabel?: ColorLabel | "";
  description?: string;
  location?: string;
  notes?: string;
}

export interface Source {
  id: number;
  path: string;
  name: string;
  isLibrary: boolean;
  addedAt: number;
  lastScanAt: number | null;
  mediaCount: number;
  totalSize: number;
  isAvailable: boolean;
}

export interface ActiveJob {
  id: number;
  jobType: string;
  phase: string;
  done: number;
  total: number;
  message: string | null;
}

export interface BackgroundJob {
  id: number;
  jobType: string;
  status: string;
  phase: string | null;
  progress: number;
  done: number;
  total: number;
  summary: Record<string, unknown> | null;
  errorMessage: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface ScanIssue {
  path: string;
  issue: string;
  detail: string | null;
  createdAt: number;
}

export interface LibraryStats {
  totalMedia: number;
  totalPhotos: number;
  totalVideos: number;
  totalAudio: number;
  totalBytes: number;
  totalSelected: number;
  totalRejected: number;
  totalFiveStar: number;
  totalFavorites: number;
  totalUnrated: number;
  totalNeedsReview: number;
  totalVideoSeconds: number;
  totalAudioSeconds: number;
  missing: number;
  metadataPending: number;
  metadataErrors: number;
  thumbnailsPending: number;
  duplicateGroups: number;
  duplicateFiles: number;
  duplicateWastedBytes: number;
  withoutCaptureDate: number;
  byType: { mediaType: MediaType; count: number; bytes: number }[];
  ratingDistribution: number[];
  byYear: { year: string; photos: number; videos: number; audio: number }[];
  topCameras: { camera: string; count: number }[];
}

export interface DuplicateGroup {
  fileHash: string;
  fileSize: number;
  items: {
    id: number;
    path: string;
    filename: string;
    mediaType: MediaType;
    rating: number;
    selectionStatus: Selection;
    sortDatetime: string | null;
  }[];
}

export type IngestLayout = "byCaptureDate" | "keepFolderStructure";

export interface IngestPlan {
  sourceDir: string;
  renameTemplate?: string | null;
  libraryDir: string;
  layout: IngestLayout;
  items: { source: string; destination: string; size: number; alreadyPresent: boolean }[];
  totalBytes: number;
  toCopy: number;
  alreadyPresent: number;
  freeBytes: number | null;
}

export interface FileOperation {
  id: number;
  batchId: string | null;
  operationType: string;
  mediaId: number | null;
  originalPath: string;
  destinationPath: string | null;
  operationStatus: string;
  errorMessage: string | null;
  result: any;
  createdAt: number;
}

export interface AppInfo {
  version: string;
  dataDir: string;
  cacheDir: string;
  databasePath: string;
  platform: string;
  tools: {
    ffmpeg: string | null;
    ffprobe: string | null;
    exiftool: string | null;
    ffmpegVersion: string | null;
    exiftoolVersion: string | null;
  };
}

export type CoreEvent =
  | { kind: "jobProgress"; jobId: number; jobType: string; status: string; phase: string; done: number; total: number; message: string | null }
  | { kind: "libraryChanged"; reason: string };

export type View =
  | { kind: "dashboard" }
  | { kind: "library"; preset: LibraryPreset }
  | { kind: "collection"; id: number }
  | { kind: "tag"; id: number }
  | { kind: "folder"; path: string; sourceId: number }
  | { kind: "duplicates" }
  | { kind: "activity" }
  | { kind: "settings" };

export type LibraryPreset = "all" | "photos" | "videos" | "audio" | "unrated" | "favorites" | "picks" | "rejected" | "review" | "recent" | "missing" | "nodate";

export type ConflictPolicy = "skip" | "addSuffix";
export type PlanStatus = "ready" | "unchanged" | "conflict" | "missing";
export interface PlanItem { mediaId: number; from: string; to: string; status: PlanStatus; message: string | null }
export interface RenamePlan { template: string; items: PlanItem[]; ready: number; conflicts: number; unchanged: number; missing: number }
export type OrganizeMode = "copy" | "move";
export interface OrganizePlan { mode: OrganizeMode; destination: string; subfolderTemplate: string | null; items: PlanItem[]; ready: number; skipped: number; totalBytes: number; freeBytes: number | null }
export interface BatchSummary { batchId: string; operationType: string; createdAt: number; completed: number; failed: number; undone: number; undoable: boolean }

export interface Segment {
  id: number; sourceMediaId: number; outputMediaId: number | null; name: string | null;
  inPoint: number; outPoint: number; actualInPoint: number | null; actualOutPoint: number | null;
  calculatedCaptureDatetime: string | null; calculatedCaptureTimezone: string | null;
  exportedPath: string | null; processingMethod: string | null; preset: string | null; exportedAt: number | null;
  verification: Record<string, any> | null; createdAt: number;
}
export type ExportPreset = "lossless" | "exact" | "compatible";
export type FileDatePolicy = "sourceFile" | "clipCapture" | "exportTime";
export interface ExportRequest { segmentIds: number[]; destination: string; preset: ExportPreset; nameTemplate: string; fileDates: FileDatePolicy; conflict: ConflictPolicy }
export interface ExportItem {
  segmentId: number; sourceMediaId: number; sourcePath: string; sourceFilename: string; clipName: string | null;
  inPoint: number; outPoint: number; actualIn: number; actualOut: number; captureLocal: string | null; captureOffset: string | null;
  to: string; status: PlanStatus; notes: string[]; message: string | null;
}
export interface ExportPlan { request: ExportRequest; items: ExportItem[]; ready: number; totalSeconds: number; method: string }
