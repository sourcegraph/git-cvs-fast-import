# 0.2.0

* Relicensed as Apache 2.0.
* CVS branches are now supported, and will be imported to Git by default.
* Migrate serialised on-disk format to `speedy`. Stores created by v0.1.0 will be migrated the first time they're read by v0.2.0.
* File errors are now fatal by default. The old warn-and-continue behaviour can be restored with the `--ignore-file-errors` flag.
* Modified CVS tags are now recreated in a way that doesn't remove their previous history in Git.

# 0.1.0

* Initial release.
