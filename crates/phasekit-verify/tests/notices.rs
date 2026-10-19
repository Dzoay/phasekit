//! The notices the crates ship (user decision MX10): phasekit-mix's Cargo licence names NIST's notice for GERG-2008's
//! coefficients, and the crate carries that notice, the repository's text, at the checksum cargo-about pins. Core
//! carries the notices of its lane ports (user decision LC1).

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

/// LC1 (PLAN.md M23.1) and IM5 (M23.2): core's lane `ln` and `expm1`, ports of the `libm` crate's from FreeBSD's msun,
/// each carry their msun file's notice at their head, and its own `exp`, a port of Arm's optimized-routines, names
/// Arm's files and licence at its head; core's NOTICE, which its package ships, reproduces LICENSE-THIRD-PARTY's
/// sections for both.
#[test]
fn phasekit_core_carries_its_ports_notices() {
    let notice = include_str!("../../phasekit-core/NOTICE");
    let third_party = include_str!("../../../LICENSE-THIRD-PARTY");
    let heads = [
        ("e_log.c", include_str!("../../phasekit-core/src/num/lanes/ln.rs"), "Developed at SunSoft"),
        ("s_expm1.c", include_str!("../../phasekit-core/src/num/lanes/expm1.rs"), "Developed at SunPro"),
        ("math/exp.c", include_str!("../../phasekit-core/src/num/elementary/exp.rs"), "Arm Limited"),
    ];
    for (origin, file, line) in heads {
        let head: Vec<&str> = file.lines().take_while(|l| l.starts_with("//") && !l.starts_with("//!")).collect();
        assert!(head.first().is_some_and(|l| l.contains(origin)), "{origin}: the head names its origin");
        assert!(head.iter().any(|l| l.contains(line)), "{origin}: its head carries `{line}`");
    }
    // A section runs from its title to the next 80-column rule, or to the end of the file.
    let rule = format!("\n{}\n", "-".repeat(80));
    for title in ["The libm crate (0.2.16) and FreeBSD's msun (Sun Microsystems)", "Arm's optimized-routines (v26.07)"]
    {
        let section = |text: &'static str| text.split_once(title).and_then(|(_, rest)| rest.split(&rule).nth(1));
        let ours = section(notice).map(str::trim_end);
        assert!(
            ours.is_some_and(|text| text.len() > 500) && ours == section(third_party).map(str::trim_end),
            "{title}"
        );
    }
}
