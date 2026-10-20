// origin: Arm's optimized-routines v26.07, math/pow.c, math/pow_common.h and math/pow_log_data.c (POW_LOG_TABLE_BITS
// 7, POW_LOG_POLY_ORDER 8, no FMA), with math/exp.c's tables through `exp`. Copyright (c) 2018-2026, Arm Limited.
// Licensed under MIT OR Apache-2.0 WITH LLVM-exception; see the crate's NOTICE.

//! `pow` (PLAN.md M23.4; user decision IM5): Arm's `pow` without FMA. x^y = exp(y·ln x), with ln x to about 68 bits
//! as hi + lo from its own 128-entry table and degree-8 polynomial, and y·(hi + lo) as ehi + elo from 27-bit halves,
//! which `exp` takes with its tail (Arm's `exp_inline`). x = 2^k·z with z in [0x1.69555p-1, 0x1.69555p0), exact, and
//! the table gives for z's subinterval 1/c, ln c and ln c's tail at a c near its centre, where 1/c has so few bits
//! that r = z/c − 1 is exact: z is split at its top 32 bits, so that each half times 1/c is exact. Arm's comments bound
//! the error at 0.54 ulp (ln's relative error 1.5·2⁻⁶⁸ without FMA, `exp`'s 0.511 ulp). k becomes a double through the
//! low bits of 1.5·2⁵² + k, never by an `as` conversion. A positive normal x with 2⁻⁶⁵ ≤ |y| < 2⁶³ whose y·ln x
//! takes `exp`'s main path takes the main path; every other case is computed beside it and selected, so a group of
//! lanes never branches on a lane: C99 Annex F's (zero, infinite and NaN arguments), a negative base, an integer power
//! of it signed by y's parity, a subnormal base, |y| below 2⁻⁶⁵ or from 2⁶³, and `exp`'s own overflow and underflow.

use super::exp;
use super::fast_two_sum;
#[cfg(any(test, feature = "libm"))]
use crate::num::Lanes;

/// ln 2 in two parts, the first with trailing zeros so that k·LN2_HI + ln c is exact.
const LN2_HI: f64 = f64::from_bits(0x3fe6_2e42_fefa_3800);
const LN2_LO: f64 = f64::from_bits(0x3d2e_f357_93c7_6730);
/// ln(1 + r) = r − ½r² + ⅓r³ − … − ⅛r⁸ on |r| < 0x1.6bp-8 (Arm: relative error 0x1.11922ap-70): A0 = −½, and A1 to A6
/// the other coefficients, near ⅓ to −⅛, times −2, −2, 4, 4, −8 and −8, as Arm's evaluation in r and A0·r² takes them.
const A0: f64 = f64::from_bits(0xbfe0_0000_0000_0000);
const A1: f64 = f64::from_bits(0xbfe5_5555_5555_5560);
const A2: f64 = f64::from_bits(0x3fe0_0000_0000_0006);
const A3: f64 = f64::from_bits(0x3fe9_9999_9959_554e);
const A4: f64 = f64::from_bits(0xbfe5_5555_5529_a47a);
const A5: f64 = f64::from_bits(0xbff2_495b_9b48_45e9);
const A6: f64 = f64::from_bits(0x3ff0_002b_8b26_3fc3);
/// The bits of 0x1.69555p-1, where z's range starts: x's bits less these hold k from bit 52 and the subinterval below
/// it.
const OFF: u64 = 0x3fe6_9555_0000_0000;
/// 1.5·2⁵²: an integer k under 2⁵¹ in magnitude added to its bits gives the double 1.5·2⁵² + k.
const SHIFT: f64 = f64::from_bits(0x4338_0000_0000_0000);
/// The masks that keep a double's top 32 bits (z's split) and its top 37 (y's and hi's 27-bit splits).
#[cfg(not(feature = "fast"))]
const TOP32: u64 = 0xffff_ffff_0000_0000;
#[cfg(not(feature = "fast"))]
const TOP37: u64 = 0xffff_ffff_f800_0000;
/// Half of [`TOP32`]'s lowest kept bit, which rounds z to its top bits.
#[cfg(not(feature = "fast"))]
const HALF32: u64 = 0x8000_0000;
/// A double's sign bit, the bits of 1, of the smallest normal number and of +∞.
const SIGN: u64 = 0x8000_0000_0000_0000;
const ONE: u64 = 0x3ff0_0000_0000_0000;
const MIN_NORMAL: u64 = 0x0010_0000_0000_0000;
const INF: u64 = 0x7ff0_0000_0000_0000;
/// |y|'s bits at the main path's bounds, 2⁻⁶⁵ and 2⁶³: below, x^y rounds to 1 ± y; from it, y is an even integer and
/// x^y is 0, 1 or ∞.
const Y_TINY: u64 = 0x3be0_0000_0000_0000;
const Y_HUGE: u64 = 0x43e0_0000_0000_0000;
/// A NaN's quiet bit.
const QUIET: u64 = 0x0008_0000_0000_0000;
/// 2⁵², which scales a subnormal x into the normals, and 52·2⁵², which takes it back off the exponent's bits.
const TWO52: f64 = f64::from_bits(0x4330_0000_0000_0000);
const EXP52: u64 = 0x0340_0000_0000_0000;

/// 1/c for each of z's 128 subintervals: j/128 or j/256 for an integer j in [128, 256) (Arm's `__pow_log_data.tab`'s
/// `invc`).
#[rustfmt::skip]
static INVC: [u64; 128] = [
    0x3ff6_a000_0000_0000, 0x3ff6_8000_0000_0000, 0x3ff6_6000_0000_0000, 0x3ff6_4000_0000_0000,
    0x3ff6_2000_0000_0000, 0x3ff6_0000_0000_0000, 0x3ff5_e000_0000_0000, 0x3ff5_c000_0000_0000,
    0x3ff5_a000_0000_0000, 0x3ff5_8000_0000_0000, 0x3ff5_6000_0000_0000, 0x3ff5_6000_0000_0000,
    0x3ff5_4000_0000_0000, 0x3ff5_2000_0000_0000, 0x3ff5_0000_0000_0000, 0x3ff4_e000_0000_0000,
    0x3ff4_c000_0000_0000, 0x3ff4_a000_0000_0000, 0x3ff4_a000_0000_0000, 0x3ff4_8000_0000_0000,
    0x3ff4_6000_0000_0000, 0x3ff4_4000_0000_0000, 0x3ff4_2000_0000_0000, 0x3ff4_0000_0000_0000,
    0x3ff4_0000_0000_0000, 0x3ff3_e000_0000_0000, 0x3ff3_c000_0000_0000, 0x3ff3_a000_0000_0000,
    0x3ff3_a000_0000_0000, 0x3ff3_8000_0000_0000, 0x3ff3_6000_0000_0000, 0x3ff3_4000_0000_0000,
    0x3ff3_4000_0000_0000, 0x3ff3_2000_0000_0000, 0x3ff3_0000_0000_0000, 0x3ff3_0000_0000_0000,
    0x3ff2_e000_0000_0000, 0x3ff2_c000_0000_0000, 0x3ff2_c000_0000_0000, 0x3ff2_a000_0000_0000,
    0x3ff2_8000_0000_0000, 0x3ff2_6000_0000_0000, 0x3ff2_6000_0000_0000, 0x3ff2_4000_0000_0000,
    0x3ff2_4000_0000_0000, 0x3ff2_2000_0000_0000, 0x3ff2_0000_0000_0000, 0x3ff2_0000_0000_0000,
    0x3ff1_e000_0000_0000, 0x3ff1_c000_0000_0000, 0x3ff1_c000_0000_0000, 0x3ff1_a000_0000_0000,
    0x3ff1_a000_0000_0000, 0x3ff1_8000_0000_0000, 0x3ff1_6000_0000_0000, 0x3ff1_6000_0000_0000,
    0x3ff1_4000_0000_0000, 0x3ff1_4000_0000_0000, 0x3ff1_2000_0000_0000, 0x3ff1_0000_0000_0000,
    0x3ff1_0000_0000_0000, 0x3ff0_e000_0000_0000, 0x3ff0_e000_0000_0000, 0x3ff0_c000_0000_0000,
    0x3ff0_c000_0000_0000, 0x3ff0_a000_0000_0000, 0x3ff0_a000_0000_0000, 0x3ff0_8000_0000_0000,
    0x3ff0_8000_0000_0000, 0x3ff0_6000_0000_0000, 0x3ff0_4000_0000_0000, 0x3ff0_4000_0000_0000,
    0x3ff0_2000_0000_0000, 0x3ff0_2000_0000_0000, 0x3ff0_0000_0000_0000, 0x3ff0_0000_0000_0000,
    0x3fef_c000_0000_0000, 0x3fef_8000_0000_0000, 0x3fef_4000_0000_0000, 0x3fef_0000_0000_0000,
    0x3fee_c000_0000_0000, 0x3fee_8000_0000_0000, 0x3fee_4000_0000_0000, 0x3fee_2000_0000_0000,
    0x3fed_e000_0000_0000, 0x3fed_a000_0000_0000, 0x3fed_6000_0000_0000, 0x3fed_4000_0000_0000,
    0x3fed_0000_0000_0000, 0x3fec_c000_0000_0000, 0x3fec_a000_0000_0000, 0x3fec_6000_0000_0000,
    0x3fec_4000_0000_0000, 0x3fec_0000_0000_0000, 0x3feb_e000_0000_0000, 0x3feb_a000_0000_0000,
    0x3feb_8000_0000_0000, 0x3feb_4000_0000_0000, 0x3feb_2000_0000_0000, 0x3fea_e000_0000_0000,
    0x3fea_c000_0000_0000, 0x3fea_a000_0000_0000, 0x3fea_6000_0000_0000, 0x3fea_4000_0000_0000,
    0x3fea_0000_0000_0000, 0x3fe9_e000_0000_0000, 0x3fe9_c000_0000_0000, 0x3fe9_a000_0000_0000,
    0x3fe9_6000_0000_0000, 0x3fe9_4000_0000_0000, 0x3fe9_2000_0000_0000, 0x3fe9_0000_0000_0000,
    0x3fe8_c000_0000_0000, 0x3fe8_a000_0000_0000, 0x3fe8_8000_0000_0000, 0x3fe8_6000_0000_0000,
    0x3fe8_4000_0000_0000, 0x3fe8_2000_0000_0000, 0x3fe7_e000_0000_0000, 0x3fe7_c000_0000_0000,
    0x3fe7_a000_0000_0000, 0x3fe7_8000_0000_0000, 0x3fe7_6000_0000_0000, 0x3fe7_4000_0000_0000,
    0x3fe7_2000_0000_0000, 0x3fe7_0000_0000_0000, 0x3fe6_e000_0000_0000, 0x3fe6_c000_0000_0000,
];

/// ln c for each subinterval, rounded to a multiple of 2⁻⁴³, so that k·LN2_HI + ln c is exact (Arm's `logc`).
#[rustfmt::skip]
static LOGC: [u64; 128] = [
    0xbfd6_2c82_f2b9_c800, 0xbfd5_d1bd_bf58_0800, 0xbfd5_7677_1745_5800, 0xbfd5_1aad_872d_f800,
    0xbfd4_be5f_9577_7800, 0xbfd4_618b_c21c_6000, 0xbfd4_0430_8686_a800, 0xbfd3_a64c_5569_4800,
    0xbfd3_47dd_9a98_8000, 0xbfd2_e8e2_bae1_2000, 0xbfd2_895a_13de_8800, 0xbfd2_895a_13de_8800,
    0xbfd2_2941_fbcf_7800, 0xbfd1_c898_c169_9800, 0xbfd1_675c_abab_a800, 0xbfd1_058b_f9ae_4800,
    0xbfd0_a324_e273_9000, 0xbfd0_4025_94b4_d000, 0xbfd0_4025_94b4_d000, 0xbfcf_b918_6d5e_4000,
    0xbfce_f0ad_cbdc_6000, 0xbfce_2707_6e2a_f000, 0xbfcd_5c21_6b4f_c000, 0xbfcc_8ff7_c79a_a000,
    0xbfcc_8ff7_c79a_a000, 0xbfcb_c286_742d_9000, 0xbfca_f3c9_4e80_c000, 0xbfca_23bc_1fe2_b000,
    0xbfca_23bc_1fe2_b000, 0xbfc9_525a_9cf4_5000, 0xbfc8_7fa0_6520_d000, 0xbfc7_ab89_0210_e000,
    0xbfc7_ab89_0210_e000, 0xbfc6_d60f_e719_d000, 0xbfc5_ff30_70a7_9000, 0xbfc5_ff30_70a7_9000,
    0xbfc5_26e5_e3a1_b000, 0xbfc4_4d2b_6ccb_8000, 0xbfc4_4d2b_6ccb_8000, 0xbfc3_71fc_201e_9000,
    0xbfc2_9552_f81f_f000, 0xbfc1_b72a_d52f_6000, 0xbfc1_b72a_d52f_6000, 0xbfc0_d77e_7cd0_9000,
    0xbfc0_d77e_7cd0_9000, 0xbfbf_ec91_31db_e000, 0xbfbe_2707_6e2b_0000, 0xbfbe_2707_6e2b_0000,
    0xbfbc_5e54_8f5b_c000, 0xbfba_926d_3a4a_e000, 0xbfba_926d_3a4a_e000, 0xbfb8_c345_d631_a000,
    0xbfb8_c345_d631_a000, 0xbfb6_f0d2_8ae5_6000, 0xbfb5_1b07_3f06_2000, 0xbfb5_1b07_3f06_2000,
    0xbfb3_41d7_961b_e000, 0xbfb3_41d7_961b_e000, 0xbfb1_6536_eea3_8000, 0xbfaf_0a30_c011_8000,
    0xbfaf_0a30_c011_8000, 0xbfab_42dd_7119_8000, 0xbfab_42dd_7119_8000, 0xbfa7_7458_f632_c000,
    0xbfa7_7458_f632_c000, 0xbfa3_9e87_b9fe_c000, 0xbfa3_9e87_b9fe_c000, 0xbf9f_829b_0e78_0000,
    0xbf9f_829b_0e78_0000, 0xbf97_b91b_07d5_8000, 0xbf8f_c0a8_b0fc_0000, 0xbf8f_c0a8_b0fc_0000,
    0xbf7f_e02a_6b10_0000, 0xbf7f_e02a_6b10_0000, 0x0000_0000_0000_0000, 0x0000_0000_0000_0000,
    0x3f80_1015_7589_0000, 0x3f90_2056_5893_8000, 0x3f98_4925_28c9_0000, 0x3fa0_415d_89e7_4000,
    0x3fa4_66ae_d42e_0000, 0x3fa8_94aa_149f_c000, 0x3fac_cb73_cddd_c000, 0x3fae_ea31_c006_c000,
    0x3fb1_973b_d146_6000, 0x3fb3_bdf5_a7d1_e000, 0x3fb5_e95a_4d97_a000, 0x3fb7_00d3_0aea_c000,
    0x3fb9_335e_5d59_4000, 0x3fbb_6ac8_8dad_6000, 0x3fbc_8858_01bc_4000, 0x3fbe_c739_830a_2000,
    0x3fbf_e891_39db_e000, 0x3fc1_178e_8227_e000, 0x3fc1_aa2b_7e23_f000, 0x3fc2_d161_0c86_8000,
    0x3fc3_65fc_b015_9000, 0x3fc4_913d_8333_b000, 0x3fc5_27e5_e4a1_b000, 0x3fc6_574e_be8c_1000,
    0x3fc6_f012_8b75_7000, 0x3fc7_898d_8544_5000, 0x3fc8_beaf_eb39_0000, 0x3fc9_5a5a_dcf7_0000,
    0x3fca_93ed_3c8a_e000, 0x3fcb_31d8_575b_d000, 0x3fcb_d087_383b_e000, 0x3fcc_6ffb_c6f0_1000,
    0x3fcd_b13d_b0d4_9000, 0x3fce_530e_ffe7_1000, 0x3fce_f5ad_e4dd_0000, 0x3fcf_991c_6cb3_b000,
    0x3fd0_7138_604d_5800, 0x3fd0_c42d_6761_6000, 0x3fd1_178e_8227_e800, 0x3fd1_6b5c_cbac_f800,
    0x3fd1_bf99_635a_6800, 0x3fd2_1445_6d0e_b800, 0x3fd2_bef0_7cdc_9000, 0x3fd3_14f1_e1d3_6000,
    0x3fd3_6b67_76be_1000, 0x3fd3_c252_7733_3000, 0x3fd4_19b4_23d5_e800, 0x3fd4_718d_c271_c800,
    0x3fd4_c9e0_9e17_3000, 0x3fd5_22ae_0738_a000, 0x3fd5_7bf7_53c8_d000, 0x3fd5_d5bd_df59_6000,
];

/// ln c − [`LOGC`]'s value for each subinterval (Arm's `logctail`): the two are ln c to within 2⁻⁹⁷.
#[rustfmt::skip]
static LOGCTAIL: [u64; 128] = [
    0x3cfa_b424_2837_5680, 0xbd1c_a508_d8e0_f720, 0xbd23_62a4_d5b6_506d, 0xbce6_84e4_9eb0_67d5,
    0xbd04_1b69_9329_3ee0, 0x3d13_d82f_484c_84cc, 0x3cdc_42f3_ed82_0b3a, 0x3d20_b1c6_8651_9460,
    0x3d25_594d_d4c5_8092, 0x3d26_7b1e_99b7_2bd8, 0x3d15_ca14_b6cf_b03f, 0x3d15_ca14_b6cf_b03f,
    0xbd16_5a24_2853_da76, 0xbd1f_afbc_68e7_5404, 0x3d1f_1fc6_3382_a8f0, 0xbd26_a8c4_fd05_5a66,
    0xbd0c_6bee_7ef4_030e, 0xbcf0_36b8_9ef4_2d7f, 0xbcf0_36b8_9ef4_2d7f, 0x3d0d_572a_ab99_3c87,
    0x3d2b_26b7_9c86_af24, 0xbd17_2f4f_543f_ff10, 0x3d21_ba91_bbca_681b, 0x3d27_794f_689f_8434,
    0x3d27_794f_689f_8434, 0x3d19_4eb0_318b_b78f, 0x3cba_4e63_3fcd_9066, 0xbd25_8c64_dc46_c1ea,
    0xbd25_8c64_dc46_c1ea, 0xbd2a_d1d9_04c1_d4e3, 0x3d2b_bdbf_7fdb_fa09, 0x3d2b_db90_7253_4a58,
    0x3d2b_db90_7253_4a58, 0xbd10_e46a_a3b2_e266, 0xbd1e_9e43_9f10_5039, 0xbd1e_9e43_9f10_5039,
    0xbd20_de8b_9007_5b8f, 0x3d17_0cc1_6135_783c, 0x3d17_0cc1_6135_783c, 0x3cf1_7886_4d27_543a,
    0xbd24_8d30_1771_c408, 0xbd2e_80a4_1811_a396, 0xbd2e_80a4_1811_a396, 0x3d0a_6996_88e8_5bf4,
    0x3d0a_6996_88e8_5bf4, 0xbd25_7554_5ca3_33f2, 0x3d2a_342c_2af0_003c, 0x3d2a_342c_2af0_003c,
    0xbd1d_0c57_585f_be06, 0x3d25_3935_e85b_aac8, 0x3d25_3935_e85b_aac8, 0x3d13_7c29_4d2f_5668,
    0x3d13_7c29_4d2f_5668, 0xbd26_9737_c933_73da, 0x3d1f_025b_61c6_5e57, 0x3d1f_025b_61c6_5e57,
    0x3d2c_5eda_ccf9_13df, 0x3d2c_5eda_ccf9_13df, 0x3d14_7c5e_768f_a309, 0x3d2d_599e_8336_8e91,
    0x3d2d_599e_8336_8e91, 0x3d1c_827a_e5d6_704c, 0x3d1c_827a_e5d6_704c, 0xbd2c_fc46_34f2_a1ee,
    0xbd2c_fc46_34f2_a1ee, 0x3cf5_02b7_f526_feaa, 0x3cf5_02b7_f526_feaa, 0xbd29_8026_7c7e_09e4,
    0xbd29_8026_7c7e_09e4, 0xbd28_8d54_93fa_a639, 0xbcdf_1e7c_f6d3_a69c, 0xbcdf_1e7c_f6d3_a69c,
    0xbd19_e23f_0dda_40e4, 0xbd19_e23f_0dda_40e4, 0x0000_0000_0000_0000, 0x0000_0000_0000_0000,
    0xbd10_c76b_999d_2be8, 0xbd23_dc5b_06e2_f7d2, 0xbd2a_a0ba_325a_0c34, 0x3d01_11c0_5cf1_d753,
    0xbd2c_1673_75bd_fd28, 0xbd19_7995_d05a_267d, 0xbd1a_68f2_47d8_2807, 0xbd0e_113e_4fc9_3b7b,
    0xbd25_325d_560d_9e9b, 0x3d2c_c85e_a5db_4ed7, 0xbd2c_6906_3c5d_1d1e, 0x3cec_1e8d_a99d_ed32,
    0x3d23_115c_3abd_47da, 0xbd13_9080_2bf7_68e5, 0x3d26_46d1_c65a_acd3, 0xbd2d_c068_afe6_45e0,
    0xbd25_34d6_4fa1_0afd, 0x3d21_ef78_ce2d_07f2, 0x3d2c_a78e_4438_9934, 0x3d03_9d6c_cb81_b4a1,
    0x3cc6_2fa8_234b_7289, 0x3d25_8379_54fd_b678, 0x3d26_33e8_e569_7dc7, 0x3d19_cf8b_2c3c_2e78,
    0xbd25_118d_e59c_21e1, 0xbd1c_6610_7091_4305, 0xbd07_3d54_aae9_2cd1, 0x3d07_f228_58a0_ff6f,
    0xbd28_7243_5056_2169, 0xbd0c_358d_4eac_e1aa, 0xbd2d_4bc4_5954_12b6, 0xbcf1_ec72_c596_2bd2,
    0xbd2a_ff2a_f715_b035, 0x3cc2_1227_6041_f430, 0xbcca_2115_65bb_8e11, 0x3d1b_cbec_ca0c_df30,
    0x3cf8_9cdb_16ed_4e91, 0x3d27_188b_163c_eae9, 0xbd2c_210e_63a5_f01c, 0x3d2b_9acd_f7a5_1681,
    0x3d2c_a6ed_5147_bdb7, 0x3d0a_87de_ba46_baea, 0x3d2a_9cfa_4a50_04f4, 0xbd28_e27a_d321_3cb8,
    0x3d11_6ecd_b0f1_77c8, 0x3d18_3b54_b606_bd5c, 0x3d08_e436_ec90_e09d, 0xbd2f_27ce_0967_d675,
    0xbd2e_2089_1b0a_d8a4, 0x3d2e_be70_8164_c759, 0x3d1f_aded_ee5d_40ef, 0xbd0a_0b2a_08a4_65dc,
];

/// Entry `i` of `table`, `i` masked to its length: the lookup cannot miss, and the compiler drops the check.
#[inline(always)]
fn entry(table: &[u64; 128], i: u64) -> f64 {
    f64::from_bits(table.get((i & 0x7f) as usize).copied().unwrap_or(0))
}

/// r = z/c − 1 and A0·r² as sq + lo3, sq what joins the sum and lo3 its error. z is split at its top 32 bits, so that
/// r = rhi + rlo exactly and sq = rhi·(A0·rhi) is exact; with the `fast` feature Arm's FMA form instead (PLAN.md M23.7).
#[inline(always)]
fn ratio(z: f64, iz: u64, invc: f64) -> (f64, f64, f64) {
    #[cfg(not(feature = "fast"))]
    {
        let zhi = f64::from_bits(iz.wrapping_add(HALF32) & TOP32);
        let zlo = z - zhi;
        let rhi = zhi * invc - 1.0;
        let rlo = zlo * invc;
        let r = rhi + rlo;
        let arhi = A0 * rhi;
        (r, rhi * arhi, rlo * (A0 * r + arhi))
    }
    #[cfg(feature = "fast")]
    {
        let _ = iz;
        let r = crate::num::fused::ratio(z, invc);
        let (sq, lo3) = crate::num::fused::square(r, A0);
        (r, sq, lo3)
    }
}

/// ln x as hi + lo, |lo| about 2⁻¹⁵ of hi's ulp, at the bits of a positive x, a subnormal one normalised with its
/// exponent below 0 in the sign bit (Arm's `log_inline` without FMA).
#[inline(always)]
fn ln_hi_lo(ix: u64) -> (f64, f64) {
    let tmp = ix.wrapping_sub(OFF);
    let i = tmp >> 45;
    let k = (tmp.cast_signed() >> 52).cast_unsigned();
    let iz = ix.wrapping_sub(tmp & 0xfff0_0000_0000_0000);
    let z = f64::from_bits(iz);
    let kd = f64::from_bits(SHIFT.to_bits().wrapping_add(k)) - SHIFT;
    let (invc, logc, logctail) = (entry(&INVC, i), entry(&LOGC, i), entry(&LOGCTAIL, i));
    let (r, sq, lo3) = ratio(z, iz, invc);
    // k·ln 2 + ln c + r.
    let t1 = kd * LN2_HI + logc;
    let (t2, lo2) = fast_two_sum(t1, r);
    let lo1 = kd * LN2_LO + logctail;
    // … + A0·r², as sq + lo3.
    let ar = A0 * r;
    let ar2 = r * ar;
    let ar3 = r * ar2;
    let (hi, lo4) = fast_two_sum(t2, sq);
    let p = ar3 * (A1 + r * A2 + ar2 * (A3 + r * A4 + ar2 * (A5 + r * A6)));
    let lo = lo1 + lo2 + lo3 + lo4 + p;
    fast_two_sum(hi, lo)
}

/// y·ln x as ehi + elo, |elo| < 2⁻²⁵·|ehi|, from [`ln_hi_lo`] at `ix`: y and hi each split at 27 bits, so that ehi
/// is their high halves' product, exact; with the `fast` feature Arm's FMA form instead (PLAN.md M23.7).
#[inline(always)]
fn exponent(ix: u64, y: f64) -> (f64, f64) {
    let (hi, lo) = ln_hi_lo(ix);
    #[cfg(not(feature = "fast"))]
    {
        let yhi = f64::from_bits(y.to_bits() & TOP37);
        let ylo = y - yhi;
        let lhi = f64::from_bits(hi.to_bits() & TOP37);
        let llo = hi - lhi + lo;
        (yhi * lhi, ylo * lhi + y * llo)
    }
    #[cfg(feature = "fast")]
    crate::num::fused::product(y, hi, lo)
}

/// Whether 2⁻⁶⁵ ≤ |y| < 2⁶³: outside, x^y is 1 ± y, or 0, 1 or ∞ for an even integer y.
#[inline(always)]
fn moderate(y: f64) -> bool {
    // Equivalent (M23.4): outside, |y·ln x| is below 2⁻⁵⁴, where `exp` gives 1 as 1 ± y rounds to, or, but at |x| = 1
    // (ln x is 0 and `exp` gives 1), from 1024 (|ln x| ≥ 2⁻⁵³), where it gives 0 or ∞ by the same signs. Arm keeps the
    // case for the directed roundings and the overflow and underflow flags.
    // equivalent mutant: replace moderate -> bool with true
    (Y_TINY..Y_HUGE).contains(&(y.to_bits() & !SIGN))
}

/// Whether `x` and `y` take the main path so far: x a positive normal number and y moderate. y·ln x must take `exp`'s
/// too.
#[inline(always)]
fn plain(x: f64, y: f64) -> bool {
    // Equivalent (M23.4): on the main path `pow_any` is the main path's value bit for bit, so taking it at every
    // `x` and `y` changes only the speed.
    // equivalent mutant: replace plain -> bool with false
    (MIN_NORMAL..INF).contains(&x.to_bits()) && moderate(y)
}

/// Whether `i` is the bits of ±0, ±∞ or a NaN (Arm's `zeroinfnan`).
#[inline(always)]
fn zero_inf_nan(i: u64) -> bool {
    (i << 1).wrapping_sub(1) >= (INF << 1).wrapping_sub(1)
}

/// Whether `v` is a signalling NaN (Arm's `issignaling_inline`, IEEE 754-2008's quiet bit).
#[inline(always)]
fn signalling(v: f64) -> bool {
    // Equivalent (M23.4): ∞'s bits and the quiet bit share no bit, so | and ^ join them alike.
    // equivalent mutant: replace | with ^
    (v.to_bits() ^ QUIET) << 1 > (INF | QUIET) << 1
}

/// Whether y, from its bits, non-zero and finite, is an integer, and whether an odd one (Arm's `checkint` as selects):
/// below 1 it is none, from 2⁵³ an even one, and between, its bits below the units' place say.
#[inline(always)]
fn checkint(iy: u64) -> (bool, bool) {
    let e = (iy >> 52) & 0x7ff;
    let units = 1_u64.wrapping_shl(0x433_u32.wrapping_sub(e as u32));
    // Equivalent (M23.4): at e = 0x433 the units' place is the last bit, so the second test holds there too.
    // equivalent mutant: replace > with >=
    let whole = e > 0x433 || (e >= 0x3ff && iy & units.wrapping_sub(1) == 0);
    let odd = (0x3ff..=0x433).contains(&e) && iy & units != 0;
    (whole, whole && odd)
}

/// |x|'s bits, a subnormal |x| normalised: scaled by 2⁵² and 52 taken from its exponent, which falls below 0 into the
/// sign bit, as [`ln_hi_lo`] takes it.
#[inline(always)]
fn normalised(x: f64) -> u64 {
    let ax = x.to_bits() & !SIGN;
    let scaled = ((x * TWO52).to_bits() & !SIGN).wrapping_sub(EXP52);
    // Equivalent (M23.4): at the smallest normal number, as at any normal one, the scaling is exact and gives |x|'s bits
    // back.
    // equivalent mutant: replace < with <=
    if ax < MIN_NORMAL { scaled } else { ax }
}

/// x^y for a finite non-zero x and |y| below 2⁻⁶⁵ or from 2⁶³, from |x|'s bits: 1 at |x| = 1, 1 ± y for a tiny y, and
/// else ∞ or 0 by whether |x| and y lie on the same side of 1 and 0.
#[inline(always)]
fn y_extreme(ax: u64, y: f64) -> f64 {
    // Equivalent (M23.4): at |x| = 1 the last select gives 1.
    // equivalent mutant: replace > with >=
    let above = ax > ONE;
    let r = if above == y.is_sign_positive() { f64::INFINITY } else { 0.0 };
    // Equivalent (M23.4): below 2⁻⁶⁵ in magnitude, 1 + y and 1 − y both round to 1; Arm keeps them for the directed
    // roundings.
    // equivalent mutant: replace + with -
    // equivalent mutant: replace - with +
    let tiny = if above { 1.0 + y } else { 1.0 - y };
    // Equivalent (M23.4): |y| = 2⁻⁶⁵ is moderate, so `pow_any` does not take this value there.
    // equivalent mutant: replace < with <=
    let r = if y.to_bits() & !SIGN < Y_TINY { tiny } else { r };
    if ax == ONE { 1.0 } else { r }
}

/// x^y for x ±0, ±∞ or NaN and y non-zero and finite: x², negated for an odd y (`odd`) and inverted for a negative
/// one, so ±0 or ±∞, or NaN.
#[inline(always)]
fn x_special(x: f64, y: f64, odd: bool) -> f64 {
    let x2 = x * x;
    let x2 = if odd { -x2 } else { x2 };
    if y.is_sign_negative() { 1.0 / x2 } else { x2 }
}

/// 1, or `nan` (x + y, a NaN) where `v` is a signalling NaN.
#[inline(always)]
fn one_unless_signalling(v: f64, nan: f64) -> f64 {
    if signalling(v) { nan } else { 1.0 }
}

/// x^y for y ±0, ±∞ or NaN, in Arm's order: 1 at y = ±0 and at x = 1, unless the other is a signalling NaN; NaN where
/// either is NaN; 1 at x = −1; 0 for |x| < 1 and y = +∞ or |x| > 1 and y = −∞; else +∞.
#[inline(always)]
fn y_special(x: f64, y: f64) -> f64 {
    let (ix, iy) = (x.to_bits(), y.to_bits());
    // Equivalent (M23.4): `sum` is taken only where x or y is a NaN, and x − y and x·y are a NaN there as x + y is (a
    // NaN's bits are not specified).
    // equivalent mutant: replace + with -
    // equivalent mutant: replace + with *
    let sum = x + y;
    let r = y * y;
    // Equivalent (M23.4): at |x| = 1 the next select gives 1.
    // equivalent mutant: replace < with <=
    let r = if (ix << 1 < ONE << 1) == y.is_sign_positive() { 0.0 } else { r };
    let r = if ix << 1 == ONE << 1 { 1.0 } else { r };
    let r = if ix << 1 > INF << 1 || iy << 1 > INF << 1 { sum } else { r };
    let r = if ix == ONE { one_unless_signalling(y, sum) } else { r };
    if iy << 1 == 0 { one_unless_signalling(x, sum) } else { r }
}

/// x^y at any `x` and `y`: the main path at |x|, a subnormal one normalised, signed by y's parity where x < 0, and the
/// cases that `x` and `y` fall in, each computed and selected in reverse of Arm's order, so that its first case wins:
/// y ±0, ±∞ or NaN; x ±0, ±∞ or NaN; NaN for x < 0 and y not an integer; |y| below 2⁻⁶⁵ or from 2⁶³. On the main path
/// it is that path's value bit for bit.
#[inline(always)]
fn pow_any(x: f64, y: f64) -> f64 {
    let (ix, iy) = (x.to_bits(), y.to_bits());
    let (whole, odd) = checkint(iy);
    let negative = x.is_sign_negative();
    let (ehi, elo) = exponent(normalised(x), y);
    let r = exp::exp_signed(ehi, elo, if negative && odd { SIGN } else { 0 });
    let r = if moderate(y) { r } else { y_extreme(ix & !SIGN, y) };
    let r = if negative && !whole { f64::NAN } else { r };
    let r = if zero_inf_nan(ix) { x_special(x, y, negative && odd) } else { r };
    if zero_inf_nan(iy) { y_special(x, y) } else { r }
}

/// x^y, under 1 ulp: Arm's `pow` without FMA (PLAN.md M23.4).
pub fn pow(x: f64, y: f64) -> f64 {
    let (ehi, elo) = exponent(x.to_bits(), y);
    if plain(x, y) && exp::plain(ehi) { exp::exp_plain(ehi, elo) } else { pow_any(x, y) }
}

/// [`pow`] on every lane at one `y`: the main path where every lane takes it, else every case on every lane.
#[cfg(any(test, feature = "libm"))]
#[inline(always)]
pub(crate) fn pow_lanes<const W: usize>(x: Lanes<W>, y: f64) -> Lanes<W> {
    let e = x.0.map(|x| exponent(x.to_bits(), y));
    let main = x.0.iter().zip(&e).all(|(&x, &(ehi, _))| plain(x, y) && exp::plain(ehi));
    if main { Lanes(e.map(|(ehi, elo)| exp::exp_plain(ehi, elo))) } else { Lanes(x.0.map(|x| pow_any(x, y))) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{LaneKernel, run_lanes};
    use crate::num::lanes::tests::{Rng, canonical};

    /// [`pow`] at one exponent as a lane kernel: the scalar function at a point, [`pow_lanes`] across lanes.
    struct Pow(f64);

    impl LaneKernel for Pow {
        type Out = u64;
        fn point(&self, x: f64, _: f64) -> u64 {
            canonical(pow(x, self.0))
        }
        fn lanes<const W: usize>(&self, x: Lanes<W>, _: Lanes<W>) -> [u64; W] {
            pow_lanes(x, self.0).0.map(canonical)
        }
    }

    /// The exponents of the lane test: the fluids' kinds (non-integer Power and α⁰ exponents, integers of both
    /// parities) and Arm's edges in y, ±0, ±∞, NaN, 2⁻⁶⁵ and 2⁶³ with their neighbours, and 2⁵³ − 1, the largest odd
    /// integer below 2⁵³.
    fn exponents() -> Vec<f64> {
        let mut y = vec![0.875, 1.5, 0.25, 2.4, -1.5, 1.0 / 3.0, 0.5, 2.0, 3.0, -1.0, -3.0, 7.0, 1075.0, 1e-300, 1e300];
        y.extend([0.0, -0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 9_007_199_254_740_991.0]);
        let edges = [Y_TINY, Y_HUGE].map(f64::from_bits);
        y.extend(edges.iter().flat_map(|&e| [e, e.next_up(), e.next_down(), -e, -e.next_up(), -e.next_down()]));
        y
    }

    /// The edges in x, each with its neighbours one ulp either side and both signs: ±0, ±1, the subnormals and the
    /// smallest normal, `f64::MAX`, ±∞, NaN, and z's range in ln, 0x1.69555p-1 to 0x1.69555p0.
    fn edges() -> Vec<f64> {
        let mut edges = vec![0.0, 1.0, 2.0, 0.5, 5e-324, 1e-310, f64::MIN_POSITIVE, f64::MAX, f64::INFINITY, f64::NAN];
        edges.extend([OFF, OFF + (1 << 52)].map(f64::from_bits));
        let signed: Vec<f64> = edges.iter().flat_map(|&e| [e, -e]).collect();
        signed.iter().flat_map(|&e| [e, e.next_up(), e.next_down()]).collect()
    }

    /// 3000 bases each over τ's and δ's range (10⁻³ to 20, whole groups on the main path), over [−20, 20] (groups
    /// that mix negative bases in), near 1 and over every bit pattern; then each edge alone among seven ordinary
    /// lanes, and the edges side by side.
    fn bases() -> Vec<f64> {
        let mut rng = Rng(23);
        let mut x: Vec<f64> = (0..3000).map(|_| rng.uniform(1e-3, 20.0)).collect();
        x.extend((0..3000).map(|_| rng.uniform(-20.0, 20.0)));
        x.extend((0..3000).map(|_| rng.uniform(0.999, 1.001)));
        x.extend((0..3000).map(|_| f64::from_bits(rng.next())));
        for e in edges() {
            x.push(e);
            x.extend((0..7).map(|_| rng.uniform(1e-3, 20.0)));
        }
        x.extend(edges());
        x
    }

    /// `Pow(y)` over `x` in groups of lanes by `run`, against [`pow`] at each point.
    fn assert_pow(x: &[f64], y: f64, run: impl Fn(&Pow, &[f64], &mut [u64]), what: &str) {
        let mut got = vec![0; x.len()];
        run(&Pow(y), x, &mut got);
        for (x, got) in x.iter().zip(got) {
            let want = pow(*x, y);
            assert!(got == canonical(want), "{what}: pow({x:e}, {y:e}) = {:e}, scalar {want:e}", f64::from_bits(got));
        }
    }

    /// PLAN.md M23.4 (VERIFICATION.md §9.2, class `Exact`): [`pow_lanes`] is [`pow`] bit for bit (NaN canonical), in
    /// every build, on 2, 4 and 8 lanes and with the `simd` feature at every dispatch level of this CPU, at each
    /// exponent of [`exponents`] over random bases (positive, mixed in sign, near 1 and every bit pattern) and the
    /// edges, alone and side by side.
    #[test]
    fn lane_pow_is_scalar_bitwise() {
        let x = bases();
        for y in exponents() {
            assert_pow(&x, y, |k, x, out| run_lanes::<_, 2>(k, x, x, out), "2 lanes");
            assert_pow(&x, y, |k, x, out| run_lanes::<_, 4>(k, x, x, out), "4 lanes");
            assert_pow(&x, y, |k, x, out| run_lanes::<_, 8>(k, x, x, out), "8 lanes");
            #[cfg(feature = "simd")]
            for level in crate::exec::tests::levels() {
                assert_pow(&x, y, |k, x, out| crate::exec::run_simd(level, k, x, x, out), &format!("{level:?}"));
            }
        }
        assert!(x.len() > 12_000 && x.iter().any(|x| x.is_nan()));
    }

    /// [`checkint`] reads y's parity from its bits: 1 and 3 odd, 2 even, 0.5, 1.5 and 2⁵² − ½ no integer, 2⁵³ − 1 odd
    /// (its units' place the last bit), 2⁵³ and 10³⁰⁰ even (no bit below the units' place), the sign ignored.
    #[test]
    fn checkint_reads_parity() {
        let cases = [
            (1.0, (true, true)),
            (3.0, (true, true)),
            (-3.0, (true, true)),
            (2.0, (true, false)),
            (0.5, (false, false)),
            (1.5, (false, false)),
            (4_503_599_627_370_495.5, (false, false)),
            (9_007_199_254_740_991.0, (true, true)),
            (9_007_199_254_740_992.0, (true, false)),
            (1e300, (true, false)),
        ];
        for (y, want) in cases {
            assert_eq!(checkint(f64::to_bits(y)), want, "checkint({y:e})");
        }
    }

    /// Arm's order for y = ±0 and x = 1: 1 even at a quiet NaN, one with a payload too, a NaN at a signalling one
    /// (IEEE 754-2008), which [`signalling`] tells by its quiet bit.
    #[test]
    fn signalling_nans_are_not_one() {
        let snan = f64::from_bits(0x7ff4_0000_0000_0000);
        let payload = f64::from_bits(0x7ff8_0000_0000_0001);
        assert!([snan, -snan, f64::from_bits(0x7ff0_0000_0000_0001)].into_iter().all(signalling));
        assert!(![f64::NAN, -f64::NAN, payload, -payload, f64::INFINITY, 1.0].into_iter().any(signalling));
        assert!(pow(snan, 0.0).is_nan() && pow(1.0, snan).is_nan());
        assert_eq!((pow(f64::NAN, -0.0), pow(1.0, f64::NAN)), (1.0, 1.0));
        assert_eq!((pow(payload, 0.0), pow(1.0, payload)), (1.0, 1.0));
    }

    /// A negative base's integer powers take y's parity for their sign, on every path: exact powers on the main path,
    /// −1 where y·ln|x| is 0, −0 and −∞ where `exp` underflows or overflows within 1024 and beyond, a subnormal, and
    /// x = −0 and −∞ themselves; a non-integer power is NaN. The values are Arm's `pow` without FMA, compiled from C.
    #[test]
    fn negative_bases_take_the_sign_of_odd_powers() {
        let cases = [
            (-2.0, 3.0, -8.0),
            (-2.0, 2.0, 4.0),
            (-1.0, 3.0, -1.0),
            (-2.0, -1073.0, -f64::from_bits(2)),
            (-2.0, -1077.0, -0.0),
            (-2.0, 1025.0, f64::NEG_INFINITY),
            (-2.0, 1501.0, f64::NEG_INFINITY),
            (-2.0, -1501.0, -0.0),
            (-0.0, 3.0, -0.0),
            (-0.0, 2.0, 0.0),
            (-0.0, -3.0, f64::NEG_INFINITY),
            (f64::NEG_INFINITY, 3.0, f64::NEG_INFINITY),
            (f64::NEG_INFINITY, -3.0, -0.0),
        ];
        for (x, y, want) in cases {
            assert_eq!(pow(x, y).to_bits(), want.to_bits(), "pow({x:e}, {y:e})");
        }
        assert!(pow(-2.0, 0.5).is_nan());
    }

    /// An odd power of a negative base is the positive base's power negated, bit for bit, also where it is subnormal:
    /// `exp`'s `specialcase` below rounds −y once, by adding and taking away −1, as it rounds y by 1. Cubes of 1000
    /// bases from 2⁻³⁴⁵ up by factors of 1.001, subnormal with 39 to 44 bits.
    #[test]
    fn odd_powers_of_negative_bases_mirror_positive_ones() {
        let mut x = f64::from_bits(0x2a60_0000_0000_0000);
        for _ in 0..1000 {
            assert_eq!(pow(-x, 3.0).to_bits(), (-pow(x, 3.0)).to_bits(), "pow({:e}, 3)", -x);
            x *= 1.001;
        }
        assert!(pow(x, 3.0) < f64::MIN_POSITIVE);
    }

    /// [`ln_hi_lo`] is Arm's `log_inline` bit for bit, hi and lo: at 1, at either end of the subinterval about 1 (r
    /// widest above 1), at z's range's ends, 1.2345, 3, 10³⁰⁰, the largest double, the smallest normal and two
    /// subnormals, normalised. The values are Arm's compiled from C, without FMA, or with it (`-mfma`, Arm's
    /// `HAVE_FAST_FMA`) for the `fast` feature: the two differ only in the last subnormal's lo.
    #[test]
    fn ln_is_arms_bitwise() {
        let last_lo = if cfg!(feature = "fast") { 0xbd03_5918_fe54_0000 } else { 0xbd03_5918_fe58_0000 };
        let cases = [
            (0x3ff0_0000_0000_0000, 0x0000_0000_0000_0000, 0x0000_0000_0000_0000),
            (0x3fef_f555_0000_0000, 0xbf55_598f_3c53_243a, 0x3bfb_4d3c_0000_0000),
            (0x3ff0_1554_ffff_ffff, 0x3f75_46d4_2562_a98b, 0x3c11_2f20_0000_0000),
            (0x3fe6_9555_0000_0000, 0xbfd6_4e37_6d38_c479, 0xbc7d_4465_4800_0000),
            (0x3ff6_9554_ffff_ffff, 0x3fd6_0e4e_90bb_af62, 0x3c60_319b_0000_0000),
            (0x3ff3_c083_126e_978d, 0x3fca_f71a_be30_f48f, 0xbc61_e4b4_9000_0000),
            (0x4008_0000_0000_0000, 0x3ff1_93ea_7aad_030b, 0xbc9a_256f_9000_0000),
            (0x7e37_e43c_8800_759c, 0x4085_9634_47f8_7fb5, 0x3d1a_bccc_0708_0000),
            (0x7fef_ffff_ffff_ffff, 0x4086_2e42_fefa_39ef, 0x3d1a_9c9e_3b39_8000),
            (0x0010_0000_0000_0000, 0xc086_232b_dd7a_bcd2, 0xbd1e_ef3f_ec1b_e000),
            (0x0000_0000_0000_0001, 0xc087_4385_446d_71c3, 0xbd28_e569_fa8e_e800),
            (0x0000_1268_8b70_e62b, 0xc086_4e69_394d_9508, last_lo),
        ];
        for (x, hi, lo) in cases {
            let (got_hi, got_lo) = ln_hi_lo(normalised(f64::from_bits(x)));
            assert_eq!((got_hi.to_bits(), got_lo.to_bits()), (hi, lo), "ln_hi_lo({:e})", f64::from_bits(x));
        }
    }
}
