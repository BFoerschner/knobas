# PostgreSQL, as knobas carries it

knobas keeps its data in a PostgreSQL server of its own. A release installer
holds that server as one file in this folder, `postgresql-<version>.tar.gz`,
and knobas unpacks it into `~/.theseus/postgresql/<version>` the first time it
starts on a machine, so a first start needs no network.

The archive is the unmodified build published at
<https://github.com/theseus-rs/postgresql-binaries/releases> for this
platform. `archives.sha256` lists the SHA-256 of each platform's archive; the
release build refuses one that does not match.

## Licenses

PostgreSQL is released under the PostgreSQL License, below. The archive holds
the license texts of everything in it (`COPYRIGHT` and `LICENSE`; on Windows
`server_license.txt` and `commandlinetools_3rd_party_licenses.txt`), and they
are unpacked beside the programs. The macOS and Windows builds include
OpenSSL 3, which is under the Apache License 2.0
(<https://www.openssl.org/source/license.html>).

    PostgreSQL Database Management System
    (also known as Postgres, formerly known as Postgres95)

    Portions Copyright (c) 1996-2026, PostgreSQL Global Development Group

    Portions Copyright (c) 1994, The Regents of the University of California

    Permission to use, copy, modify, and distribute this software and its
    documentation for any purpose, without fee, and without a written agreement
    is hereby granted, provided that the above copyright notice and this
    paragraph and the following two paragraphs appear in all copies.

    IN NO EVENT SHALL THE UNIVERSITY OF CALIFORNIA BE LIABLE TO ANY PARTY FOR
    DIRECT, INDIRECT, SPECIAL, INCIDENTAL, OR CONSEQUENTIAL DAMAGES, INCLUDING
    LOST PROFITS, ARISING OUT OF THE USE OF THIS SOFTWARE AND ITS
    DOCUMENTATION, EVEN IF THE UNIVERSITY OF CALIFORNIA HAS BEEN ADVISED OF THE
    POSSIBILITY OF SUCH DAMAGE.

    THE UNIVERSITY OF CALIFORNIA SPECIFICALLY DISCLAIMS ANY WARRANTIES,
    INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY
    AND FITNESS FOR A PARTICULAR PURPOSE.  THE SOFTWARE PROVIDED HEREUNDER IS
    ON AN "AS IS" BASIS, AND THE UNIVERSITY OF CALIFORNIA HAS NO OBLIGATIONS TO
    PROVIDE MAINTENANCE, SUPPORT, UPDATES, ENHANCEMENTS, OR MODIFICATIONS.
