# Dependency review for 0.3.6

This review was performed on 2026-11-17 while preparing the release. GitHub's
open Dependabot, CodeQL and secret-scanning alerts were checked, along with
open pull requests. A separate [OSV query](https://google.github.io/osv.dev/post-v1-querybatch/)
covered the Cargo lockfile and identifiable vendored JavaScript packages:
GitHub Dependabot does not track the copied browser bundles in `ui/vendor`.
These checks identify known advisories; they do not establish that the
application or its dependencies have no vulnerabilities.

## Updates

The initial scan flagged 14 package versions. Available compatible fixes were
applied, including packages whose vulnerable configuration is not used by
Cadenza. Browser distributions were obtained from the official npm tarballs,
verified against npm's SHA-512 integrity values, and copied without executing
package scripts. Their provenance is recorded beside the bundles.

| Package | Previous | Updated | Advisory or reason |
| --- | --- | --- | --- |
| ammonia | 4.2.4 | 4.2.5 | [RUSTSEC-2026-0533](https://rustsec.org/advisories/RUSTSEC-2026-0533.html), HTML namespace sanitization |
| rustix | 1.1.5 | 1.1.6 | [RUSTSEC-2026-0486](https://rustsec.org/advisories/RUSTSEC-2026-0486.html), extended-attribute buffer initialization |
| rustls-webpki | 0.103.15 | 0.103.16 | [RUSTSEC-2026-0382](https://rustsec.org/advisories/RUSTSEC-2026-0382.html), distrusted certificate intermediates |
| webpki-root-certs | 1.0.9 | 1.0.10 | [RUSTSEC-2026-0383](https://rustsec.org/advisories/RUSTSEC-2026-0383.html), root certificate removal |
| tokio | 1.53.2 | 1.53.3 | [RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html), AsyncFd I/O safety |
| plist / its quick-xml dependency | 1.8.0 / 0.38.4 | 1.10.4 / 0.42.0 | [RUSTSEC-2026-0194](https://rustsec.org/advisories/RUSTSEC-2026-0194.html) and [0195](https://rustsec.org/advisories/RUSTSEC-2026-0195.html), XML resource exhaustion |
| KaTeX | 0.16.47 | 0.18.2 | [GHSA-238p-pmpm-9mq7](https://github.com/KaTeX/KaTeX/security/advisories/GHSA-238p-pmpm-9mq7), inherited renderer options |
| Mermaid | 12.1.0 | 12.3.0 | Updates bundled DOMPurify from 3.4.12 to 3.4.17 and bundled KaTeX to 0.18.11 |

DOMPurify 3.4.17 addresses the five returned advisories:
[GHSA-55q2-fjhq-7xh7](https://github.com/advisories/GHSA-55q2-fjhq-7xh7),
[GHSA-6688-9rhm-gjv2](https://github.com/advisories/GHSA-6688-9rhm-gjv2),
[GHSA-c6gr-qvv4-5v3f](https://github.com/advisories/GHSA-c6gr-qvv4-5v3f),
[GHSA-cg2g-xh64-v4q7](https://github.com/advisories/GHSA-cg2g-xh64-v4q7) and
[GHSA-mrv2-3pqr-j5jr](https://github.com/advisories/GHSA-mrv2-3pqr-j5jr).
Mermaid's bundled versions were verified from its official source map.

The KaTeX regression was reproduced with the previous bundle: an inherited
`trust: true` option enabled an unsafe link. The updated bundle rejects it,
and the real-browser math suite now tests this case. All 20 KaTeX WOFF2 fonts
remain byte-identical, preserving the fonts shared with MathLive.

## Remaining advisories

The post-update OSV query covered 663 package/version pairs: 592 registry
packages from `Cargo.lock` and 71 npm package/version pairs, including the
versions identifiable in Mermaid's source map. It returned six affected
package versions, listed below. No advisories were returned for the 71 npm
pairs. This is not a complete inventory of every transitive dependency in
every vendored bundle or of native operating-system libraries.

| Package | Status and disposition |
| --- | --- |
| glib 0.18.5 | [RUSTSEC-2024-0429](https://rustsec.org/advisories/RUSTSEC-2024-0429.html) / GHSA-wrw7-89jp-8q8g describe unsound `VariantStrIter` methods. Tauri's GTK3/WebKitGTK dependency family requires glib 0.18; the published fix is in 0.20+. Searches of Cadenza and locally resolved Rust dependency sources found the affected type and its `array_iter_str` constructor only inside glib itself. No application call path was identified. The dependency remains affected by version; upgrading glib alone would not replace the GTK3 dependency family. Reassess when updating Tauri's Linux webview stack. |
| chrono 0.4.45 | [RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543.html): maintenance notice, without a security fix version. Transitive dependency. |
| proc-macro-error 1.0.4 | [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html): unmaintained build-time dependency. |
| zvariant 5.9.2, zbus_names 4.3.1, zvariant_derive 5.9.2 | [RUSTSEC-2026-0370](https://rustsec.org/advisories/RUSTSEC-2026-0370.html), [0371](https://rustsec.org/advisories/RUSTSEC-2026-0371.html), [0372](https://rustsec.org/advisories/RUSTSEC-2026-0372.html): maintenance notices following consolidation into zbus 6. Migration depends on upstream consumers; these notices do not report a security exploit. |

The remaining advisories were documented rather than silently ignored or
removed from scanner output. Repository-local evidence is retained under
the ignored `qa-results/release-0.3.6/` directory, including OSV queries,
responses, full advisory details, distribution integrity values and test logs.

## Validation

After the dependency updates, Linux Rust tests (273), Clippy, all six browser
suites, and the PDF and PowerPoint export regressions were rerun. The math
suite includes the new inherited-options regression. Windows verification
and the final release builds are separate CI gates; their final results are
recorded with the release preparation evidence.

To repeat the local checks, follow `CONTRIBUTING.md`. For dependency checks,
submit each registry package's name/version from `src-tauri/Cargo.lock` to
OSV with ecosystem `crates.io`, and the pinned browser package versions with
ecosystem `npm`. Preserve the raw results, inspect advisory details and
withdrawal status, and evaluate call paths before classifying applicability.
Review `ui/vendor/*/README.md` and official distribution source maps when
updating copied bundles; Dependabot's Cargo and Actions coverage does not
replace that review.
