// Development-only: lets the UI render in a plain browser (no Tauri) with fake data,
// for layout work and screenshots. Never included when running inside the app.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

export function installDevMock() {
  mockWindows("main");
  const items = Array.from({ length: 180 }, (_, i) => ({
    id: i + 1,
    mediaType: i % 9 === 0 ? "video" : i % 23 === 0 ? "audio" : "photo",
    filename: i % 9 === 0 ? `GX01${String(i).padStart(4, "0")}.MP4` : `IMG_${String(4000 + i)}.JPG`,
    sortDatetime: `2026-07-${String(1 + (i % 28)).padStart(2, "0")}T15:${String(i % 60).padStart(2, "0")}:00`,
    captureKnown: true, width: 4032, height: 3024, duration: i % 9 === 0 ? 37 + i : null,
    rating: i % 7 === 0 ? 5 : i % 3, selectionStatus: i % 11 === 0 ? "selected" : i % 13 === 0 ? "rejected" : "none",
    reviewStatus: i % 17 === 0 ? "needs_review" : "none", favorite: i % 10 === 0, colorLabel: i % 8 === 0 ? "green" : null,
    thumbnailStatus: "done", isMissing: false, fileSize: 3_000_000 + i * 1000,
  }));
  const stats = {
    totalMedia: 2937, totalPhotos: 2450, totalVideos: 380, totalAudio: 95, totalBytes: 147_400_000_000, totalSelected: 312, totalRejected: 120,
    totalFiveStar: 88, totalFavorites: 40, totalUnrated: 2100, totalNeedsReview: 14, totalVideoSeconds: 41_000, totalAudioSeconds: 20_000,
    missing: 3, metadataPending: 0, metadataErrors: 2, thumbnailsPending: 0, duplicateGroups: 7, duplicateFiles: 15, duplicateWastedBytes: 420_000_000,
    withoutCaptureDate: 12,
    byType: [{ mediaType: "video", count: 380, bytes: 126_700_000_000 }, { mediaType: "photo", count: 2450, bytes: 18_400_000_000 }, { mediaType: "audio", count: 95, bytes: 2_300_000_000 }],
    ratingDistribution: [2100, 120, 210, 260, 159, 88],
    byYear: [{ year: "2024", photos: 400, videos: 60, audio: 10 }, { year: "2025", photos: 900, videos: 120, audio: 40 }, { year: "2026", photos: 1150, videos: 200, audio: 45 }],
    topCameras: [{ camera: "Apple iPhone 16 Pro", count: 1400 }, { camera: "GoPro HERO12 Black", count: 600 }, { camera: "SONY ILCE-7M4", count: 520 }, { camera: "DJI Mini 4 Pro", count: 90 }],
  };
  const sources = [{ id: 1, path: "/Volumes/Media/2026", name: "2026", isLibrary: false, addedAt: 0, lastScanAt: Date.now(), mediaCount: 2937, totalSize: stats.totalBytes, isAvailable: true }];
  // Placeholder images instead of the thumb:// / media:// protocols.
  (window as any).__TAURI_INTERNALS__.convertFileSrc = (id: string) => {
    const n = Number(id) || 1;
    const hue = (n * 47) % 360;
    const svg = `<svg xmlns='http://www.w3.org/2000/svg' width='400' height='300'><defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'><stop offset='0' stop-color='hsl(${hue},55%,62%)'/><stop offset='1' stop-color='hsl(${(hue + 40) % 360},50%,30%)'/></linearGradient></defs><rect width='400' height='300' fill='url(%23g)'/><path d='M0 300 L120 170 L200 240 L290 140 L400 300Z' fill='rgba(0,0,0,.35)'/><circle cx='320' cy='70' r='28' fill='rgba(255,255,255,.7)'/></svg>`;
    return `data:image/svg+xml;utf8,${svg.replace(/#/g, "%23")}#`;
  };
  mockIPC((cmd, args: any) => {
    switch (cmd) {
      case "stats": return stats;
      case "list_sources": return sources;
      case "list_collections": return [{ id: 1, name: "Brazil 2026 Highlights", description: null, collectionType: "manual", rules: null, mediaCount: 42, createdAt: 0 }, { id: 2, name: "Five-star videos", description: null, collectionType: "smart", rules: { minRating: 5 }, mediaCount: 12, createdAt: 0 }];
      case "list_tags": return [{ id: 1, name: "Activities", parentId: null, path: "Activities", mediaCount: 0 }, { id: 2, name: "Windsurfing", parentId: 1, path: "Activities / Windsurfing", mediaCount: 120 }, { id: 3, name: "Locations", parentId: null, path: "Locations", mediaCount: 0 }];
      case "folders": return [["/Volumes/Media/2026", 10], ["/Volumes/Media/2026/Brazil", 400], ["/Volumes/Media/2026/Windsurfing", 300]];
      case "cameras": return stats.topCameras.map((c) => c.camera);
      case "active_jobs": return [{ id: 3, jobType: "index:1", phase: "Creating thumbnails", done: 1240, total: 2937, message: null }];
      case "recent_jobs": return [{ id: 2, jobType: "index", status: "completed", phase: "Done", progress: 1, done: 0, total: 0, summary: { new: 2937 }, errorMessage: null, createdAt: Date.now(), updatedAt: Date.now() }];
      case "query_media": return { total: items.length, offset: args.query.offset, items: items.slice(args.query.offset, args.query.offset + args.query.limit) };
      case "media_detail": {
        const it = items.find((x) => x.id === args.id)!;
        return { ...it, currentFilename: it.filename, originalFilename: it.filename, originalPath: "/Volumes/Media/2026/" + it.filename, currentPath: "/Volumes/Media/2026/" + it.filename, folderPath: "/Volumes/Media/2026/Brazil", extension: "jpg", captureDatetime: it.sortDatetime, captureTimezone: "-03:00", captureTimeSource: "exif_original", filesystemCreationTime: Date.now(), filesystemModificationTime: Date.now(), addedAt: Date.now(), cameraMake: "Apple", cameraModel: "iPhone 16 Pro", lens: "24mm f/1.78", exposure: "1/500s  f/1.8  ISO 50  24mm", gpsLatitude: -27.59, gpsLongitude: -48.54, metadataStatus: "done", tags: [{ id: 2, name: "Windsurfing", parentId: 1, path: "Activities / Windsurfing", mediaCount: 0 }], collections: [{ id: 1, name: "Brazil 2026 Highlights" }], derivedSegments: 0 };
      }
      case "get_app_info": return { version: "0.1.0", dataDir: "~/Library/Application Support/app.storyloom.desktop", cacheDir: "~/Library/Caches/app.storyloom.desktop", databasePath: "~/Library/Application Support/app.storyloom.desktop/catalog.sqlite", platform: "macos", tools: { ffmpeg: "/opt/homebrew/bin/ffmpeg", ffprobe: "/opt/homebrew/bin/ffprobe", exiftool: null, ffmpegVersion: "8.1.2", exiftoolVersion: null } };
      case "list_backups": return [];
      case "duplicates": return [];
      case "file_operations": return [];
      default: return null;
    }
  });
}
