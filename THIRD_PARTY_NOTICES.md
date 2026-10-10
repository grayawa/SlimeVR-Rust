# Third-party notices

This is an attribution and license-scope index for the principal inherited
components and embedded assets. Each build's complete dependency inventory
is determined by its target, features and lockfiles. Preserve the accompanying
license texts and use each locked dependency's source and license when
redistributing a build.

| Material                                                  | Source / copyright                                                                                        | License / notice                                                                             |
| --------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| SlimeVR Server code, React interface and inherited assets | Eiren Rain and SlimeVR Contributors; upstream baseline `83941fd38e91cc91ca6b360deab5c2ae986dd1b6`         | `LICENSE-MIT`, `LICENSE-APACHE`                                                              |
| SolarXR Protocol / generated bindings                     | [SlimeVR/SolarXR-Protocol](https://github.com/SlimeVR/SolarXR-Protocol), pinned submodule                 | MIT OR Apache-2.0; submodule licenses                                                        |
| GPUI Kit                                                  | [GPUI Kit](https://gpui-kit.com/), 0.7.1                                                                  | Apache-2.0; `gui-gpui/assets/GPUI-Kit-LICENSE-APACHE`                                        |
| GPUI Windows vendor                                       | Zed contributors, `gpui-pre-windows` 0.3.8                                                                | Apache-2.0; vendor license and `SLIMEVR-PATCH.md`                                            |
| Linux StatusNotifier tray                                 | [iovxw/ksni](https://github.com/iovxw/ksni), 0.3.3                                                        | Unlicense; `gui-gpui/assets/ksni-UNLICENSE`                                                  |
| OpenVR SDK / API library                                  | Copyright (c) 2015 Valve Corporation                                                                      | BSD-3-Clause; `bindings-provider/openvr/LICENSE`                                             |
| jMonkeyEngine math                                        | Copyright (c) 2009–2012 jMonkeyEngine                                                                     | BSD-3-Clause; `server-rust/licenses/jMonkeyEngine-BSD`                                       |
| ktmath reference / adaptations                            | Donald F Reynolds / ktmath                                                                                | MIT OR Apache-2.0; `server-rust/licenses/ktmath/`                                            |
| Poppins, Noto Sans, Lexend, OpenDyslexic fonts            | Respective font authors                                                                                   | SIL OFL-1.1; `gui-gpui/assets/fonts/*-OFL.txt`                                               |
| Ubuntu font                                               | Canonical and Ubuntu font contributors                                                                    | Ubuntu Font Licence; `gui-gpui/assets/fonts/Ubuntu-UFL.txt`                                  |
| Twemoji graphics                                          | Copyright 2019 Twitter, Inc and other contributors; [jdecked/twemoji](https://github.com/jdecked/twemoji) | CC-BY-4.0; `licenses/assets/Twemoji-CC-BY-4.0.txt` and asset attribution                     |
| `@twemoji/svg` packaging / optimization code              | Copyright (c) 2023 Samuel Kopp                                                                            | MIT; its package license, separate from the underlying graphics                              |
| SlimeVR SteamVR driver                                    | SlimeVR contributors; pinned by driver download script                                                    | Original MIT / Apache-2.0 notices supplied with the driver                                   |
| Microsoft VC runtime DLLs                                 | Microsoft                                                                                                 | Original redistributable terms; `VC-Runtime-LICENSE.rtf` and source provenance in the bundle |

Font sources and conversions are recorded in `gui-gpui/assets/fonts/SOURCES.md`.
Fonts and graphics retain their separate asset licenses. Twemoji graphics use
CC-BY-4.0; their npm packaging wrapper uses MIT. Include the graphics attribution
when redistributing those graphics.

Rust dependencies are pinned in each Cargo.lock; npm dependencies are pinned in
pnpm-lock.yaml. An `OR` expression permits choosing a compatible alternative;
`AND` requires satisfying both sets of terms. MPL-2.0 dependencies retain their
source/notice obligations. Build tools, separate data assets, SteamVR, WebView2
and Windows retain their own terms. Generated protocol bindings retain the
protocol's notices.
