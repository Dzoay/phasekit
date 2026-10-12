//! The blobs (PLAN.md M2.4): every record round-trips through blob v1 bitwise, and every EOS value is hashed.

use super::*;

/// The record as its blob ships it: restricted metadata is not written (D14).
fn shipped(record: &FluidRecord) -> FluidRecord {
    let mut shipped = record.clone();
    shipped.environmental = None;
    shipped
}

/// PLAN.md M2.4: every fluid's record round-trips through blob v1 bitwise (decode(encode(r)) == r, and the bytes
/// re-encode identically), restricted metadata aside.
#[test]
fn every_fluid_round_trips_bitwise() {
    let records = records();
    for (_, record) in &records {
        let blob = record.encode();
        let back = FluidRecord::decode(&blob).unwrap_or_else(|e| panic!("{}: {e}", record.name));
        assert_eq!(back, shipped(record), "{}", record.name);
        assert_eq!(back.encode(), blob, "{}", record.name);
        assert_eq!(crate::datagen::blob(record).as_ref(), Ok(&blob), "{}", record.name);
    }
    assert_eq!(records.len(), 136);
    // Datagen's own check refuses a record its blob does not reproduce (`applied` is runtime state).
    let mut applied = records[0].1.clone();
    applied.applied = vec!["DIV-0001".into()];
    assert_eq!(crate::datagen::blob(&applied), Err("1-Butene: the blob decodes to a different record".to_string()));
}

/// Every value of an EOS: its constants and every term field, as mutable places (u8 exponents apart).
fn every_value(e: &mut EosRecord) -> (Vec<&mut f64>, Vec<&mut u8>) {
    let (mut f, mut u): (Vec<&mut f64>, Vec<&mut u8>) = (Vec::new(), Vec::new());
    f.extend([&mut e.gas_constant, &mut e.t_reducing, &mut e.rho_reducing, &mut e.rho_max]);
    for p in &mut e.power {
        f.extend([&mut p.n, &mut p.t, &mut p.c]);
        u.extend([&mut p.d, &mut p.l]);
    }
    for p in &mut e.lemmon2005 {
        f.extend([&mut p.n, &mut p.t, &mut p.m]);
        u.extend([&mut p.d, &mut p.l]);
    }
    for p in &mut e.double_exponential {
        f.extend([&mut p.n, &mut p.t, &mut p.gd, &mut p.gt, &mut p.lt]);
        u.extend([&mut p.d, &mut p.ld]);
    }
    for p in &mut e.gaussian {
        f.extend([&mut p.n, &mut p.t, &mut p.eta, &mut p.epsilon, &mut p.beta, &mut p.gamma]);
        u.push(&mut p.d);
    }
    for p in &mut e.gao_b {
        f.extend([&mut p.n, &mut p.t, &mut p.eta, &mut p.epsilon, &mut p.beta, &mut p.gamma, &mut p.b]);
        u.push(&mut p.d);
    }
    for p in &mut e.non_analytic {
        f.extend([&mut p.n, &mut p.a, &mut p.b, &mut p.beta, &mut p.big_a, &mut p.big_b, &mut p.big_c, &mut p.big_d]);
    }
    for term in &mut e.ideal {
        match term {
            IdealTerm::Lead { a1, a2 } | IdealTerm::Offset { a1, a2, .. } => f.extend([a1, a2]),
            IdealTerm::LogTau { a } => f.push(a),
            IdealTerm::Power { n, t } => f.extend([n, t]),
            IdealTerm::PlanckEinstein { n, theta } => f.extend([n, theta]),
            IdealTerm::PlanckEinsteinGeneralized { n, theta, c, d } => f.extend([n, theta, c, d]),
            IdealTerm::Cp0Power { c, t, tc, t0 } => f.extend([c, t, tc, t0]),
            _ => panic!("an ideal-gas kind this test does not know"),
        }
    }
    (f, u)
}

/// E14 on real records (the seed's `every_eos_field_is_hashed`, PLAN.md M2.4): moving any value of any of the 136
/// EOS by one ulp (an exponent by one) changes the EOS hash, and the shape hash too unless the value is R, ρ_r or
/// ρ_max.
#[test]
fn every_eos_field_is_hashed_on_real_records() {
    let mut checked = 0;
    for (_, record) in records() {
        let (base, shape) = (record.eos.eos_hash(), record.eos.shape_hash());
        let mut probe = record.eos.clone();
        let (floats, ints) = every_value(&mut probe);
        let (nf, ni) = (floats.len(), ints.len());
        for k in 0..nf + ni {
            let mut eos = record.eos.clone();
            let (mut floats, mut ints) = every_value(&mut eos);
            if k < nf {
                *floats[k] = f64::from_bits(floats[k].to_bits() + 1);
            } else {
                *ints[k - nf] ^= 1;
            }
            let scale = matches!(k, 0 | 2 | 3); // R, rho_r, rho_max
            assert_ne!(eos.eos_hash(), base, "{} value {k}", record.name);
            assert_eq!(eos.shape_hash() != shape, !scale, "{} value {k}", record.name);
            checked += 1;
        }
    }
    assert!(checked > 10_000, "{checked}");
}
