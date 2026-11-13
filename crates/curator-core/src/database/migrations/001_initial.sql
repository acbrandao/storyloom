-- Media Curator catalog schema, version 1.
-- Conventions:
--   * Instants from the filesystem are stored as INTEGER unix milliseconds (UTC).
--   * Capture date/times are stored as the *local wall-clock* time written by the
--     camera ("YYYY-MM-DDTHH:MM:SS[.fff]") plus a separate UTC offset ("+02:00"),
--     which is NULL when the device did not record one. We never invent an offset.
--   * Original (extracted) values and user corrections are stored in separate columns.

CREATE TABLE sources (
    id              INTEGER PRIMARY KEY,
    path            TEXT NOT NULL UNIQUE,
    name            TEXT NOT NULL,
    is_library      INTEGER NOT NULL DEFAULT 0,   -- 1 = managed library folder created by an ingest copy
    added_at        INTEGER NOT NULL,
    last_scan_at    INTEGER
);

CREATE TABLE media (
    id                            INTEGER PRIMARY KEY,
    source_id                     INTEGER REFERENCES sources(id) ON DELETE SET NULL,
    original_filename             TEXT NOT NULL,
    current_filename              TEXT NOT NULL,
    original_path                 TEXT NOT NULL,
    current_path                  TEXT NOT NULL UNIQUE,
    folder_path                   TEXT NOT NULL,
    extension                     TEXT NOT NULL,
    media_type                    TEXT NOT NULL CHECK (media_type IN ('photo','video','audio')),
    mime_type                     TEXT,
    file_size                     INTEGER NOT NULL,
    file_hash                     TEXT,                     -- BLAKE3, computed in the background
    -- capture time as extracted (never overwritten by user edits)
    capture_datetime              TEXT,
    capture_timezone              TEXT,
    capture_time_source           TEXT,
    -- user correction (takes precedence for display/sort when set)
    user_capture_datetime         TEXT,
    user_capture_timezone         TEXT,
    -- effective local capture time used for sorting/filtering (maintained by the app)
    sort_datetime                 TEXT,
    filesystem_creation_time      INTEGER,
    filesystem_modification_time  INTEGER NOT NULL,
    width                         INTEGER,
    height                        INTEGER,
    orientation                   INTEGER,
    duration                      REAL,
    codec                         TEXT,
    audio_codec                   TEXT,
    frame_rate                    REAL,
    bitrate                       INTEGER,
    sample_rate                   INTEGER,
    camera_make                   TEXT,
    camera_model                  TEXT,
    lens                          TEXT,
    exposure                      TEXT,                     -- human readable summary, e.g. "1/250s f/8 ISO 100 24mm"
    gps_latitude                  REAL,
    gps_longitude                 REAL,
    gps_altitude                  REAL,
    title                         TEXT,
    artist                        TEXT,
    album                         TEXT,
    genre                         TEXT,
    embedded_caption              TEXT,
    embedded_keywords             TEXT,
    -- curation (all independent properties)
    rating                        INTEGER NOT NULL DEFAULT 0 CHECK (rating BETWEEN 0 AND 5),
    selection_status              TEXT NOT NULL DEFAULT 'none' CHECK (selection_status IN ('none','selected','rejected')),
    review_status                 TEXT NOT NULL DEFAULT 'none' CHECK (review_status IN ('none','needs_review','reviewed')),
    favorite                      INTEGER NOT NULL DEFAULT 0,
    color_label                   TEXT CHECK (color_label IN ('red','orange','yellow','green','blue','purple')),
    description                   TEXT,
    location                      TEXT,
    notes                         TEXT,
    -- processing state
    metadata_status               TEXT NOT NULL DEFAULT 'pending' CHECK (metadata_status IN ('pending','done','error')),
    metadata_error                TEXT,
    thumbnail_status              TEXT NOT NULL DEFAULT 'pending' CHECK (thumbnail_status IN ('pending','done','error','unsupported')),
    is_missing                    INTEGER NOT NULL DEFAULT 0,
    added_at                      INTEGER NOT NULL,
    last_scanned_at               INTEGER NOT NULL,
    updated_at                    INTEGER NOT NULL
);

CREATE INDEX idx_media_type            ON media(media_type);
CREATE INDEX idx_media_sort_datetime   ON media(sort_datetime);
CREATE INDEX idx_media_rating          ON media(rating);
CREATE INDEX idx_media_selection       ON media(selection_status);
CREATE INDEX idx_media_folder          ON media(folder_path);
CREATE INDEX idx_media_source          ON media(source_id);
CREATE INDEX idx_media_added           ON media(added_at);
CREATE INDEX idx_media_size            ON media(file_size);
CREATE INDEX idx_media_hash            ON media(file_hash);
CREATE INDEX idx_media_metadata_status ON media(metadata_status);
CREATE INDEX idx_media_camera          ON media(camera_model);

-- Full-text index over searchable text (filenames, descriptions, camera, tags, folder).
CREATE VIRTUAL TABLE media_fts USING fts5(content, tokenize = 'unicode61 remove_diacritics 2');

CREATE TABLE tags (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    parent_id   INTEGER REFERENCES tags(id) ON DELETE CASCADE,
    created_at  INTEGER NOT NULL
);
CREATE UNIQUE INDEX idx_tags_parent_name ON tags(COALESCE(parent_id, 0), name COLLATE NOCASE);

CREATE TABLE media_tags (
    media_id    INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    tag_id      INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (media_id, tag_id)
);
CREATE INDEX idx_media_tags_tag ON media_tags(tag_id);

CREATE TABLE collections (
    id               INTEGER PRIMARY KEY,
    name             TEXT NOT NULL,
    description      TEXT,
    collection_type  TEXT NOT NULL CHECK (collection_type IN ('manual','smart')),
    rules_json       TEXT,                 -- smart collections: serialized MediaQuery filter
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);

CREATE TABLE collection_media (
    collection_id   INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    media_id        INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    sequence_order  INTEGER NOT NULL DEFAULT 0,
    added_at        INTEGER NOT NULL,
    PRIMARY KEY (collection_id, media_id)
);
CREATE INDEX idx_collection_media_media ON collection_media(media_id);

CREATE TABLE projects (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL,
    description  TEXT,
    date_start   TEXT,
    date_end     TEXT,
    location     TEXT,
    cover_media_id INTEGER REFERENCES media(id) ON DELETE SET NULL,
    notes        TEXT,
    export_settings_json TEXT,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE project_media (
    id              INTEGER PRIMARY KEY,
    project_id      INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    media_id        INTEGER REFERENCES media(id) ON DELETE CASCADE,
    segment_id      INTEGER REFERENCES video_segments(id) ON DELETE CASCADE,
    sequence_order  INTEGER NOT NULL DEFAULT 0,
    scene_name      TEXT,
    caption         TEXT,
    notes           TEXT,
    CHECK (media_id IS NOT NULL OR segment_id IS NOT NULL)
);
CREATE INDEX idx_project_media_project ON project_media(project_id, sequence_order);

-- Provenance of extracted clips: a clip always points back at its source.
CREATE TABLE video_segments (
    id                           INTEGER PRIMARY KEY,
    source_media_id              INTEGER NOT NULL REFERENCES media(id) ON DELETE RESTRICT,
    output_media_id              INTEGER REFERENCES media(id) ON DELETE SET NULL,
    name                         TEXT,
    in_point                     REAL NOT NULL,
    out_point                    REAL NOT NULL,
    calculated_capture_datetime  TEXT,
    calculated_capture_timezone  TEXT,
    exported_path                TEXT,
    processing_method            TEXT CHECK (processing_method IN ('stream_copy','reencode')),
    created_at                   INTEGER NOT NULL,
    CHECK (out_point > in_point)
);
CREATE INDEX idx_video_segments_source ON video_segments(source_media_id);

-- Journal of every filesystem-modifying operation (copy, rename, move, export, trim).
CREATE TABLE file_operations (
    id                 INTEGER PRIMARY KEY,
    batch_id           TEXT,
    operation_type     TEXT NOT NULL,
    media_id           INTEGER REFERENCES media(id) ON DELETE SET NULL,
    original_path      TEXT NOT NULL,
    destination_path   TEXT,
    previous_metadata  TEXT,         -- JSON: timestamps/size/hash before the operation
    result_metadata    TEXT,         -- JSON: verification results after the operation
    operation_status   TEXT NOT NULL CHECK (operation_status IN ('pending','completed','failed','undone')),
    error_message      TEXT,
    created_at         INTEGER NOT NULL,
    completed_at       INTEGER
);
CREATE INDEX idx_file_operations_batch ON file_operations(batch_id);

-- Immutable snapshot of everything we extracted from the file the first time we saw it.
CREATE TABLE metadata_snapshots (
    id                      INTEGER PRIMARY KEY,
    media_id                INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    original_metadata_json  TEXT NOT NULL,
    extractor               TEXT NOT NULL,
    extracted_at            INTEGER NOT NULL
);
CREATE INDEX idx_metadata_snapshots_media ON metadata_snapshots(media_id);

CREATE TABLE background_jobs (
    id             INTEGER PRIMARY KEY,
    job_type       TEXT NOT NULL,
    status         TEXT NOT NULL CHECK (status IN ('queued','running','completed','failed','cancelled')),
    phase          TEXT,
    progress       REAL NOT NULL DEFAULT 0,
    done           INTEGER NOT NULL DEFAULT 0,
    total          INTEGER NOT NULL DEFAULT 0,
    payload_json   TEXT,
    summary_json   TEXT,
    error_message  TEXT,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);

CREATE TABLE scan_issues (
    id          INTEGER PRIMARY KEY,
    job_id      INTEGER REFERENCES background_jobs(id) ON DELETE CASCADE,
    path        TEXT NOT NULL,
    issue       TEXT NOT NULL,     -- 'inaccessible' | 'unsupported' | 'corrupt' | 'empty'
    detail      TEXT,
    created_at  INTEGER NOT NULL
);

CREATE TABLE settings (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);
