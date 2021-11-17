use std::time::{SystemTime, UNIX_EPOCH};

use git_cvs_fast_import_process::Output;
use git_cvs_fast_import_state::Manager;
use git_fast_import::{CommitBuilder, FileCommand, Identity, Mark};

pub(crate) struct Processor {
    state: Manager,
    output: Output,
    identity: Identity,
}

enum Parent {
    PreviousTag(Mark),
    FileContent { mark: Mark, time: SystemTime },
    None,
}

impl Processor {
    pub(crate) fn new(state: &Manager, output: &Output, identity: Identity) -> Self {
        Self {
            state: state.clone(),
            output: output.clone(),
            identity,
        }
    }

    pub(crate) async fn process(&self, tag: &[u8]) -> anyhow::Result<()> {
        // For each tag, we need to fake a Git commit with the correct content,
        // since CVS tags don't map onto Git tags especially gracefully, then
        // send a relevant tag.
        //
        // The tricky part here is knowing what the parent commit should be:
        // different CVS file revisions might have different patchsets as their
        // logical parents! Since this is essentially unsolvable without
        // splitting tags into per-file tags (which obfuscates the underlying
        // CVS tag), we'll use a heuristic: the _last_ patchset that any
        // revision in the tag belongs to will be the parent.

        let tag_str = String::from_utf8_lossy(tag).into_owned();
        let mut parent = Parent::None;
        log::trace!("processing tag {}", &tag_str);

        let file_revision_iter = self.state.get_file_revisions_for_tag(tag).await;
        let file_revision_ids = match file_revision_iter.iter() {
            Some(ids) => ids,
            None => {
                log::debug!("tag {} does not have any file revisions", &tag_str);
                return Ok(());
            }
        };

        // If this tag has already been seen previously, then there will be a
        // previous fake commit. Let's see if there is, and then we can figure
        // out if the content has changed.
        if let Some(mark) = self.state.get_mark_for_tag(tag).await {
            // Grab the patchset content and compare it to what we have now.
            let patchset = self.state.get_patchset_from_mark(&mark).await?;
            if &patchset.file_revisions == file_revision_ids {
                // Nothing to do here; continue.
                log::trace!("not changing tag {}, as content matches", &tag_str);
                return Ok(());
            }

            // Since it doesn't match, we'll have to create a new fake commit,
            // but we need to parent it on the previous commit so we can shift
            // the tag.
            parent = Parent::PreviousTag(mark);
        }

        let mut builder = CommitBuilder::new(format!("refs/heads/tags/{}", &tag_str));
        builder
            .committer(self.identity.clone())
            .message(format!("Fake commit for tag {}.", &tag_str));

        // Unlike regular commits, we'll remove all the file content and
        // then attach the new content that is known to be on the tag. This
        // means that Git will have to figure out what the diffs look like.
        builder.add_file_command(FileCommand::DeleteAll);

        let mut time = UNIX_EPOCH;
        for file_revision_id in file_revision_ids.iter() {
            let file_revision = self
                .state
                .get_file_revision_by_id(*file_revision_id)
                .await?;

            match file_revision.mark {
                Some(mark) => builder.add_file_command(FileCommand::Modify {
                    mode: git_fast_import::Mode::Normal,
                    mark: mark.into(),
                    path: file_revision.key.path.clone(),
                }),
                None => builder.add_file_command(FileCommand::Delete {
                    path: file_revision.key.path.clone(),
                }),
            };

            if file_revision.time > time {
                time = file_revision.time;
            }

            if let Parent::PreviousTag(_) = parent {
                continue;
            }

            // If we're still calculating the parent, then we'll have to find
            // out what patchset this file revision is in, if any, and check if
            // it's newer than what we've seen.
            if let Some((patchset_mark, patchset)) = self
                .state
                .get_last_patchset_for_file_revision(*file_revision_id)
                .await
            {
                match parent {
                    Parent::PreviousTag(_) => {
                        // Nothing to do, since we have a previous tag to parent
                        // on.
                    }
                    Parent::FileContent {
                        mark: _mark,
                        time: parent_time,
                    } => {
                        if parent_time < patchset.time {
                            parent = Parent::FileContent {
                                mark: patchset_mark,
                                time: patchset.time,
                            };
                        }
                    }
                    Parent::None => {
                        parent = Parent::FileContent {
                            mark: patchset_mark,
                            time: patchset.time,
                        };
                    }
                }
            }
        }

        // Set the parent commit, if any.
        match parent {
            Parent::PreviousTag(mark) => {
                log::trace!(
                    "tag {} is parented on previous tag commit {}",
                    &tag_str,
                    mark
                );
                builder.from(mark);
            }
            Parent::FileContent { mark, time: _time } => {
                log::trace!(
                    "tag {} is parented on commit {} based on file content",
                    &tag_str,
                    mark
                );
                builder.from(mark);
            }
            Parent::None => {}
        }

        // Now we can send the commit.
        let mark = self.output.commit(builder.build()?).await?;
        self.state
            .add_patchset(mark, tag, &time, file_revision_ids.iter().copied())
            .await;

        // Since file_revision_iter is still holding a read lock on the tag
        // state, we need to drop it before saving the mark.
        drop(file_revision_iter);

        self.state.add_tag_mark(tag, mark).await;

        // And we can tag the commit.
        self.output.lightweight_tag(&tag_str, mark).await?;

        Ok(())
    }
}
