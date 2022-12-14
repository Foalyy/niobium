DROP TABLE IF EXISTS photo;

CREATE TABLE photo (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    filename VARCHAR(255),
    path VARCHAR(255),
    uid VARCHAR(16),
    md5 VARCHAR(32),
    sort_order INTEGER DEFAULT 0,
    hidden INTEGER DEFAULT 0,
    metadata_parsed INTEGER DEFAULT 0,
    width INTEGER NOT NULL DEFAULT 0,
    height INTEGER NOT NULL DEFAULT 0,
    color VARCHAR(16) NOT NULL DEFAULT '',
    title VARCHAR(255) NOT NULL DEFAULT '',
    place VARCHAR(255) NOT NULL DEFAULT '',
    date_taken VARCHAR(16) NOT NULL DEFAULT '',
    camera_model VARCHAR(255) NOT NULL DEFAULT '',
    lens_model VARCHAR(255) NOT NULL DEFAULT '',
    focal_length VARCHAR(16) NOT NULL DEFAULT '',
    aperture VARCHAR(16) NOT NULL DEFAULT '',
    exposure_time VARCHAR(16) NOT NULL DEFAULT '',
    sensitivity VARCHAR(16) NOT NULL DEFAULT ''
);