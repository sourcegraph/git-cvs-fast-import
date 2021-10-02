CREATE TABLE file_revision_commits (
    id INTEGER PRIMARY KEY,
    path TEXT NOT NULL,
    revision TEXT NOT NULL,
    mark INTEGER NULL,
    author TEXT NOT NULL,
    message TEXT NOT NULL,
    time INTEGER NOT NULL
);

CREATE UNIQUE INDEX
    file_revision_commits_unique_idx
ON
    file_revision_commits (path, revision);


CREATE TABLE file_revision_commit_branches (
    file_revision_commit_id INTEGER NOT NULL,
    branch TEXT NOT NULL,
    PRIMARY KEY (file_revision_commit_id, branch),
    FOREIGN KEY (file_revision_commit_id)
        REFERENCES file_revision_commits (id)
        ON DELETE RESTRICT
        ON UPDATE RESTRICT
);


CREATE TABLE tags (
    id INTEGER PRIMARY KEY,
    tag TEXT NOT NULL,
    file_revision_commit_id INTEGER NOT NULL,
    FOREIGN KEY (file_revision_commit_id)
        REFERENCES file_revision_commits (id)
        ON DELETE RESTRICT
        ON UPDATE RESTRICT
);


CREATE TABLE patchsets (
    id INTEGER PRIMARY KEY,
    mark INTEGER NOT NULL,
    branch TEXT NOT NULL,
    time INTEGER NOT NULL
);


CREATE TABLE file_revision_commit_patchsets (
    file_revision_commit_id INTEGER NOT NULL,
    patchset_id INTEGER NOT NULL,
    PRIMARY KEY (file_revision_commit_id, patchset_id),
    FOREIGN KEY (file_revision_commit_id)
        REFERENCES file_revision_commits (id)
        ON DELETE RESTRICT
        ON UPDATE RESTRICT,
    FOREIGN KEY (patchset_id)
        REFERENCES patchsets (id)
        ON DELETE RESTRICT
        ON UPDATE RESTRICT
);


CREATE TABLE marks (
    raw BLOB NOT NULL
);