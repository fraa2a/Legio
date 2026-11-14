# Security policy

## Reporting a vulnerability

Report suspected vulnerabilities through GitHub private security advisories for this repository. Do not open a public issue for an undisclosed vulnerability and do not include secrets, credentials, tokens, private keys, or sensitive user data in public discussions.

Include the affected version or revision, the operating system, reproduction steps, impact, and any known mitigation. Maintainers will coordinate validation, remediation, and disclosure through the private advisory.

## Supported versions

Legio is under active development. Security fixes target the current supported release line and the default branch when no stable release exists.

## Verifying releases

Release files carry GitHub build provenance attestations, a `SHA256SUMS.txt` file and CycloneDX SBOMs. Verify a download with `gh attestation verify <file> --repo fraa2a/Legio` and compare checksums as described in [releases and updates](_meta/docs/UPDATING.md#verifying-a-download).
