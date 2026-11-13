//! Data structures shared between the core and the UI (serialized as camelCase JSON).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Photo,
    Video,
    Audio,
}

impl MediaType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaType::Photo => "photo",
            MediaType::Video => "video",
            MediaType::Audio => "audio",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "photo" => Some(Self::Photo),
            "video" => Some(Self::Video),
            "audio" => Some(Self::Audio),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub is_library: bool,
    pub added_at: i64,
    pub last_scan_at: Option<i64>,
    pub media_count: i64,
    pub total_size: i64,
    pub is_available: bool,
}

/// Compact row for the virtualized grid.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaSummary {
    pub id: i64,
    pub media_type: MediaType,
    pub filename: String,
    pub sort_datetime: Option<String>,
    /// False when `sort_datetime` is only a provisional file date (capture time unknown).
    pub capture_known: bool,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration: Option<f64>,
    pub rating: i64,
    pub selection_status: String,
    pub review_status: String,
    pub favorite: bool,
    pub color_label: Option<String>,
    pub thumbnail_status: String,
    pub is_missing: bool,
    pub file_size: i64,
}

/// Every catalog field for one item (inspector panel).
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaDetail {
    pub id: i64,
    pub source_id: Option<i64>,
    pub original_filename: String,
    pub current_filename: String,
    pub original_path: String,
    pub current_path: String,
    pub folder_path: String,
    pub extension: String,
    pub media_type: String,
    pub mime_type: Option<String>,
    pub file_size: i64,
    pub file_hash: Option<String>,
    pub capture_datetime: Option<String>,
    pub capture_timezone: Option<String>,
    pub capture_time_source: Option<String>,
    pub user_capture_datetime: Option<String>,
    pub user_capture_timezone: Option<String>,
    pub sort_datetime: Option<String>,
    pub filesystem_creation_time: Option<i64>,
    pub filesystem_modification_time: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub orientation: Option<i64>,
    pub duration: Option<f64>,
    pub codec: Option<String>,
    pub audio_codec: Option<String>,
    pub frame_rate: Option<f64>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens: Option<String>,
    pub exposure: Option<String>,
    pub gps_latitude: Option<f64>,
    pub gps_longitude: Option<f64>,
    pub gps_altitude: Option<f64>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub embedded_caption: Option<String>,
    pub embedded_keywords: Option<String>,
    pub rating: i64,
    pub selection_status: String,
    pub review_status: String,
    pub favorite: bool,
    pub color_label: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub metadata_status: String,
    pub metadata_error: Option<String>,
    pub thumbnail_status: String,
    pub is_missing: bool,
    pub added_at: i64,
    pub last_scanned_at: i64,
    pub tags: Vec<Tag>,
    pub collections: Vec<CollectionRef>,
    pub derived_segments: i64,
    /// Set when this item is a clip exported from another video.
    pub clip_origin: Option<ClipOrigin>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClipOrigin {
    pub segment_id: i64,
    pub source_media_id: i64,
    pub source_filename: String,
    pub in_point: f64,
    pub out_point: f64,
    pub actual_in_point: Option<f64>,
    pub processing_method: Option<String>,
    pub preset: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub parent_id: Option<i64>,
    /// Full path, e.g. "Locations / Brazil / Rio de Janeiro".
    pub path: String,
    pub media_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionRef {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub collection_type: String,
    pub rules: Option<MediaFilter>,
    pub media_count: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SortField {
    #[default]
    CaptureDate,
    Filename,
    Rating,
    FileSize,
    AddedAt,
    Duration,
}

/// Filter criteria. All present criteria are AND-ed together. Also the rule format of smart collections.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MediaFilter {
    pub text: Option<String>,
    pub media_types: Vec<MediaType>,
    pub min_rating: Option<i64>,
    pub max_rating: Option<i64>,
    pub unrated_only: bool,
    pub selection_status: Option<String>,
    pub review_status: Option<String>,
    pub favorite_only: bool,
    pub color_label: Option<String>,
    /// Media must carry every listed tag (or one of its descendants).
    pub tag_ids: Vec<i64>,
    pub collection_id: Option<i64>,
    pub source_id: Option<i64>,
    /// Folder prefix (includes subfolders).
    pub folder: Option<String>,
    /// Inclusive local dates, "YYYY-MM-DD".
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub camera: Option<String>,
    pub min_duration: Option<f64>,
    pub max_duration: Option<f64>,
    pub min_width: Option<i64>,
    pub added_since_ms: Option<i64>,
    pub missing_only: bool,
    /// Items whose capture time is unknown (no metadata and no user correction).
    pub no_capture_date: bool,
    pub has_location: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct MediaQuery {
    pub filter: MediaFilter,
    pub sort: SortField,
    pub descending: bool,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaPage {
    pub total: i64,
    pub offset: i64,
    pub items: Vec<MediaSummary>,
}

/// Partial update of curation fields for one or many items. `None` = leave unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CurationUpdate {
    pub rating: Option<i64>,
    pub selection_status: Option<String>,
    pub review_status: Option<String>,
    pub favorite: Option<bool>,
    /// Some("") clears the label.
    pub color_label: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundJob {
    pub id: i64,
    pub job_type: String,
    pub status: String,
    pub phase: Option<String>,
    pub progress: f64,
    pub done: i64,
    pub total: i64,
    pub summary: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
