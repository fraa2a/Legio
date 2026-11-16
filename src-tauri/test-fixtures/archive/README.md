Archive extraction test fixtures.

`safe.rar` is decoded from `libarchive/test/test_read_format_rar5_stored.rar.uu` in the [libarchive test suite](https://github.com/libarchive/libarchive/blob/master/libarchive/test/test_read_format_rar5_stored.rar.uu), licensed under the libarchive BSD license. The ZIP fixtures were generated with Python `zipfile`; the 7z fixtures with 7-Zip.

The multi-volume RAR fixtures come from the test files of [rarfile](https://github.com/markokr/rarfile) 4.5 (`test/files`), copyright Marko Kreen, ISC license: `rar3-old.rar`, `rar3-old.r00` and `rar3-old.r01` are RAR4 volumes with legacy names; `rar5-vols.part1.rar` to `rar5-vols.part3.rar` are RAR5 volumes. Both sets hold `vols/bigfile.txt` (205,000 bytes, CRC32 `509ad74c`) split across volumes and `vols/smallfile.txt` (2,050 bytes, CRC32 `d08a1f86`).
