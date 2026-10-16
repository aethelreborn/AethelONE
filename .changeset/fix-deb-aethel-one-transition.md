---
default: patch
---

Fix upgrading the Linux .deb when the 2.8.0 build (Debian package `aethel-one`) is installed: the 2.14.0 and 2.15.0 packages had lost the legacy `Replaces`/`Conflicts` fields, so dpkg refused to overwrite `/usr/bin/oneclient_app` and the install failed. Both packaging workflows now declare the `aethel-one` transition again, and a guard fails the build if a package ever Conflicts with itself.
