# 0.2.0

* Relicensed as Apache 2.0.
* Migrate serialised on-disk format to `speedy`. This means that existing stores **must be recreated**.
* File errors are now fatal by default. The old warn-and-continue behaviour can be restored with the `--ignore-file-errors` flag.

# 0.1.0

* Initial release.
