-- Phase 3: clip extraction details.
ALTER TABLE video_segments ADD COLUMN actual_in_point REAL;      -- where the exported clip really starts (keyframe for lossless)
ALTER TABLE video_segments ADD COLUMN actual_out_point REAL;
ALTER TABLE video_segments ADD COLUMN preset TEXT;
ALTER TABLE video_segments ADD COLUMN exported_at INTEGER;
ALTER TABLE video_segments ADD COLUMN verification_json TEXT;    -- duration / timestamp / embedded-metadata checks
ALTER TABLE video_segments ADD COLUMN updated_at INTEGER;
CREATE INDEX idx_video_segments_output ON video_segments(output_media_id);
