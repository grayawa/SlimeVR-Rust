# Licensing

Original contributions and modifications maintained by SlimeVR Rust are licensed
under **GPL-3.0-or-later**: the GNU General Public License, version 3 or, at your
option, any later version. The complete version 3 text is in [LICENSE](LICENSE).
This applies to our Rust backend, native GPUI frontend, OpenVR overlay wrapper,
Tauri host, changes to the React frontend, tests, build scripts and documentation,
unless a file or the exceptions below specify different terms.

Inherited portions of SlimeVR Server remain available under their original
MIT / Apache-2.0 dual license. They can be combined with GPLv3 contributions;
the resulting project is distributed under GPL-3.0-or-later while retaining
all applicable upstream notices. [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE) are upstream notices, not an alternative
permissive license for this project's GPL-only additions.

## Material with separate licenses

- `solarxr-protocol/`: upstream MIT OR Apache-2.0 submodule, including its own
  third-party FlatBuffers notices.
- `bindings-provider/openvr/`: Valve's BSD-3-Clause submodule.
- `gui-gpui/vendor/gpui-pre-windows/`: Apache-2.0, including the documented local
  patch; this vendor tree intentionally retains its existing license.
- Inherited React code, translations, images, sounds and icons retain their
  upstream terms and attribution. Our modifications are GPL-3.0-or-later unless
  specifically licensed otherwise.
- Mathematical implementations retain the jMonkeyEngine BSD-3-Clause and ktmath
  MIT / Apache-2.0 notices in `server-rust/licenses/`.
- Fonts, Twemoji graphics, dependencies, bundled drivers, OpenVR and Microsoft
  runtime binaries retain their own licenses; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

Do not remove or replace these copyright notices or third-party license files.
The GPL grant does not grant rights to the SlimeVR name or logo; the retained
[TRADEMARK.md](TRADEMARK.md) explains the upstream trademark policy. This project
is independent and unofficial.

## Distribution and contributions

When distributing GPL-covered binaries, provide their complete corresponding
source and the scripts needed to build them, in accordance with GPL section 6.
The public repository, its recorded build commit, recursive submodule revisions,
lockfiles and build instructions identify the source used for our builds.
Portable bundles carry `SOURCE-CODE.txt` with these references and a reminder to
provide any local changes. A link to a different version of the code is not a
substitute for the source of the binary being distributed.

Private use or modification does not by itself require publication. Commercial
use and redistribution are permitted subject to the license conditions.
Contributions to project-owned code are accepted under GPL-3.0-or-later unless
explicitly agreed otherwise; changes inside separately licensed vendor code
retain that code's license. See [CONTRIBUTING.md](CONTRIBUTING.md).

Previously distributed revisions keep any MIT / Apache-2.0 permissions already
granted. This change does not revoke those permissions or relicense upstream
work on behalf of its authors. GPL-only changes cannot be contributed back to a
permissively licensed upstream without the relevant rights holders also granting
terms acceptable to that upstream.
