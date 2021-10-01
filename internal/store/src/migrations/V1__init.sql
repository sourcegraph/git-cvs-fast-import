CREATE TABLE file_revisions (
    id INTEGER PRIMARY KEY,
    path BLOB NOT NULL,
    revision TEXT NOT NULL,
    time INTEGER NOT NULL,
    mark INTEGER NULL
);

CREATE UNIQUE INDEX file_revisions_unique_idx ON file_revisions (path, revision);

CREATE INDEX file_revisions_mark_idx ON file_revisions (mark);

CREATE INDEX file_revisions_time_idx ON file_revisions (time);


CREATE TABLE file_revision_branches (
    id INTEGER PRIMARY KEY,
    file_revision INTEGER NOT NULL,
    branch BLOB NOT NULL,
    FOREIGN KEY (file_revision)
        REFERENCES file_revisions (id)
        ON DELETE RESTRICT
        ON UPDATE RESTRICT
);

CREATE UNIQUE INDEX file_revision_branches_unique_idx ON file_revision_branches (file_revision, branch);


CREATE TABLE tags (
    id INTEGER PRIMARY KEY,
    tag TEXT NOT NULL,
    file BLOB NOT NULL,
    revision TEXT NOT NULL
);

CREATE UNIQUE INDEX tags_unique_idx ON tags (tag, file, revision);

CREATE INDEX tags_file_revision_idx ON tags (file, revision);

CREATE INDEX tags_tag_idx ON tags (tag);


CREATE TABLE patchsets (
    mark INTEGER PRIMARY KEY,
    branch BLOB NOT NULL,
    time INTEGER NOT NULL
);


CREATE TABLE patchset_file_revisions (
    id INTEGER PRIMARY KEY,
    patchset INTEGER NOT NULL,
    file_revision INTEGER NOT NULL,
    FOREIGN KEY (patchset)
        REFERENCES patchsets (mark)
        ON DELETE RESTRICT
        ON UPDATE RESTRICT,
    FOREIGN KEY (file_revision)
        REFERENCES file_revisions (id)
        ON DELETE RESTRICT
        ON UPDATE RESTRICT
);

CREATE UNIQUE INDEX patchset_file_revisions_unique_idx ON patchset_file_revisions (patchset, file_revision);


CREATE TABLE marks (
    raw BLOB NOT NULL
);