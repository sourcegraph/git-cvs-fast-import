# git-cvs-fast-import

`git-cvs-fast-import` provides a restartable, performant Git importer for CVS repositories, focused on providing continuously updated mirrors of CVS repositories so that they can be analysed using existing tools that can only handle Git natively, such as [Sourcegraph](https://sourcegraph.com).

To support continuous updates, `git-cvs-fast-import` makes some tradeoffs compared to other tools that convert CVS repositories into Git: most notably, considerably less effort is made to preserve precise history, especially on branches and tags. This is not intended to be a general purpose, one time converter when migrating from a CVS setup to Git: see [the comparison to other tools](#comparison-to-other-tools) for suggestions on what to use in that case.

## Installation

Linux binaries are provided on [the releases page](https://github.com/sourcegraph/git-cvs-fast-import/releases), including RPM and DEB packages for RHEL/CentOS and Debian/Ubuntu installs, respectively. These binaries have been tested back to CentOS 7 and Ubuntu 16.04.

You will also need `git` installed, as `git-cvs-fast-import` uses the [`git fast-import`](https://git-scm.com/docs/git-fast-import) command internally when operating. Any version released in the last decade should be sufficient.

## Usage

You will need access to the `CVSROOT` of the CVS repository you wish to import, as `git-cvs-fast-import` parses the RCS files in the root to import the history of each file. In practice, this means you should expect to see a tree of files ending in `,v`.

You will also need a valid Git repository. This means that you need to `git init` your target repository before running `git-cvs-fast-import` for the first time.

Full help is available through `git-cvs-fast-import --help`, but for most uses, you only need to provide the CVSROOT, Git repository, metadata store, and (optionally) the CVS directories to be imported. For example, to import the `project` and `src` directories from a CVS repository at `/cvs`, and write to a Git repository at `/git`, and store the metadata at `/tmp/import.db`, you would run the following:

```sh
git-cvs-fast-import -c /cvs -g /git -s /tmp/import.db project src
```

By default, all branches will be imported, but this can be controlled by only specifying the branches of interest with `--branch`.

## Comparison to other tools

We know of three other tools that allow for CVS-to-Git conversion:

* [`git-cvsimport`](https://git-scm.com/docs/git-cvsimport): this ships with Git, and has support for incremental updates.
* [`cvs-fast-export`](https://gitlab.com/esr/cvs-fast-export): this is a standalone tool that parses CVS repositories and exports data in the `git fast-import` stream format, but does not support incremental updates.
* `cvs2git` is referenced in the `git-cvsimport` manpage, but no longer appears to have a home page.

We would suggest trying `cvs-fast-export` first for one-time conversions where the CVS repository will not be used thereafter, and then falling back to `git-cvsimport` if `cvs-fast-export` fails (which can happen with complex Git histories).

## Known issues

* Tag history can be misleading: CVS tags are applied on a per-file basis, whereas Git tags are per-repository. As a result, `git-cvs-fast-import` makes a fake commit for each tag: this ensures that the actual content of the tag is correct, but may be misleading in terms of the history of the tag if the same CVS tag was applied to different files at different times, as commits may appear in the Git log that weren't logically part of the CVS history for a specific file.

## Development

Please refer to [`DEVELOPMENT.md`](DEVELOPMENT.md) for more detail on how this tool is structured, why some choices were made, and how to contribute.
