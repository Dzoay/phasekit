//! The notices the crates ship (user decision MX10): phasekit-mix's Cargo licence names NIST's notice for GERG-2008's
//! coefficients, and the crate carries that notice, the repository's text, at the checksum cargo-about pins.

use phasekit_verify::sha256_hex;

/// MX10: `crates/phasekit-mix/LICENSE-NIST-GERG` is `LICENSES/LicenseRef-NIST-GERG.txt` byte for byte, so the published
/// crate carries the notice REUSE records; phasekit-mix's `license` is `(MIT OR Apache-2.0) AND LicenseRef-NIST-GERG`;
/// and `about.toml` clarifies the crate's file at that text's sha256, so cargo-about refuses a copy that drifts.
#[test]
fn phasekit_mix_carries_nists_notice() {
    let notice = include_str!("../../phasekit-mix/LICENSE-NIST-GERG");
    assert_eq!(notice, include_str!("../../../LICENSES/LicenseRef-NIST-GERG.txt"));
    let manifest = include_str!("../../phasekit-mix/Cargo.toml");
    assert!(manifest.lines().any(|l| l == r#"license = "(MIT OR Apache-2.0) AND LicenseRef-NIST-GERG""#));
    let checksum = format!(r#"checksum = "{}""#, sha256_hex(notice.as_bytes()));
    assert!(include_str!("../../../about.toml").lines().any(|l| l == checksum), "{checksum}");
}
