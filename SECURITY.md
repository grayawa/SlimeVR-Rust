# Security policy

SlimeVR-Rust is an independently maintained development preview. Security fixes
target the current `main` branch. Reports should identify the exact commit and
affected build, operating system and configuration.

## Reporting

Use [GitHub private vulnerability reporting](https://github.com/grayawa/SlimeVR-Rust/security/advisories/new)
when the repository's **Report a vulnerability** entry is available. Include the
affected component, reproduction steps, impact and a minimal example. Use
placeholders for credentials and personal data when preparing examples.

If that entry is unavailable, open a **Private security contact request** issue
containing a request for a private conversation. Keep its public description
limited to contact arrangements. Share the vulnerability details through the
private channel agreed with the maintainer.

The maintainer assesses reproducibility and impact, prepares a fix and coordinates
disclosure with the reporter. Ordinary crashes and tracking problems use the
bug report template with sanitized logs.

## Deployment assumptions

Desktop hosts start the backend API on `127.0.0.1:21110`. Connected API clients
can change settings and perform reset, device management and firmware operations.
Keep this interface on a trusted local endpoint. Remote access requires a trusted
network and access control supplied by a VPN or authenticated proxy.

Tracker UDP traffic uses the local network; device approval identifies accepted
devices. Treat Wi-Fi provisioning credentials, configuration files, diagnostic
logs and recordings as personal data when sharing them.
