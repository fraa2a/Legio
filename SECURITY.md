# Security policy

## Reporting a vulnerability

Report suspected vulnerabilities through GitHub private security advisories for this repository. Do not open a public issue for an undisclosed vulnerability and do not include secrets, credentials, tokens, private keys, or sensitive user data in public discussions.

Include the affected version or revision, the operating system, reproduction steps, impact, and any known mitigation. Maintainers will coordinate validation, remediation, and disclosure through the private advisory.

## Supported versions

Only the latest released version receives fixes, including security fixes. Older versions do not receive fixes: update to the latest release through the built-in updater, or through the package manager for the AUR package.

The minimum supported database schema is the one created by any released version. Migrations start from schema version 0 and upgrade in sequence to the current version. Downgrading to an older version is not supported: Legio refuses to migrate a database whose schema is newer than its build supports.
