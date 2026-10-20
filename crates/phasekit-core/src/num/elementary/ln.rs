// origin: Arm's optimized-routines v26.07, math/log.c and math/log_data.c (LOG_TABLE_BITS 7, LOG_POLY_ORDER 6,
// LOG_POLY1_ORDER 12, no FMA). Copyright (c) 2018-2025, Arm Limited. Licensed under MIT OR Apache-2.0 WITH
// LLVM-exception; see the crate's NOTICE.

//! `ln` (PLAN.md M23.3; user decision IM5): Arm's `log`, its two 128-entry tables and degree-6 polynomial, without
//! FMA. x = 2^k·z with z in [0x1.6p-1, 0x1.6p0), exact, and the table gives for z's subinterval 1/c and ln c at a c
//! near its centre, and c as chi + clo, so that r = (z − chi − clo)·(1/c) ≈ z/c − 1 rounds about once, as FMA's would
//! (the second table stands in for FMA). ln x = k·ln 2 + ln c + r, summed in hi + lo, plus ln(1 + r) − r, a
//! polynomial in r. Near 1, from 1 − 2⁻⁴ to 1 + 0x1.09p-4, it is a degree-12 polynomial in r = x − 1 instead, whose
//! r − r²/2 is summed exactly with r split at 2²⁷. Arm's comments bound the error at about 0.52 ulp on the main path
//! and 0.507 near 1. k becomes a double through the low bits of 1.5·2⁵² + k, never by an `as` conversion. A positive
//! normal x away from 1 takes the main path; every other case is computed beside it and selected, so a group of lanes
//! never branches on a lane.

use super::{fast_two_sum, split};
#[cfg(any(test, feature = "libm"))]
use crate::num::Lanes;

/// ln 2 in two parts, the first with trailing zeros so that k·LN2_HI + ln c is exact.
const LN2_HI: f64 = f64::from_bits(0x3fe6_2e42_fefa_3800);
const LN2_LO: f64 = f64::from_bits(0x3d2e_f357_93c7_6730);
/// ln(1 + r) − r = A0·r² + … + A4·r⁶ on |r| < 0x1.fp-9 (Arm: abs error 0x1.882ff33p-65).
const A0: f64 = f64::from_bits(0xbfe0_0000_0000_0001);
const A1: f64 = f64::from_bits(0x3fd5_5555_5551_305b);
const A2: f64 = f64::from_bits(0xbfcf_ffff_ffeb_4590);
const A3: f64 = f64::from_bits(0x3fc9_99b3_24f1_0111);
const A4: f64 = f64::from_bits(0xbfc5_5575_e506_c89f);
/// Near 1, ln(1 + r) = r + B0·r² + … + B10·r¹² on [−2⁻⁴, 0x1.09p-4] (Arm: relative error 0x1.c04d76cp-63). B0 is −½.
const B0: f64 = f64::from_bits(0xbfe0_0000_0000_0000);
const B1: f64 = f64::from_bits(0x3fd5_5555_5555_5577);
const B2: f64 = f64::from_bits(0xbfcf_ffff_ffff_fdcb);
const B3: f64 = f64::from_bits(0x3fc9_9999_9995_dd0c);
const B4: f64 = f64::from_bits(0xbfc5_5555_5567_45a7);
const B5: f64 = f64::from_bits(0x3fc2_4924_a344_de30);
const B6: f64 = f64::from_bits(0xbfbf_ffff_a442_3d65);
const B7: f64 = f64::from_bits(0x3fbc_7184_282a_d6ca);
const B8: f64 = f64::from_bits(0xbfb9_99eb_43b0_68ff);
const B9: f64 = f64::from_bits(0x3fb7_8182_f7af_d085);
const B10: f64 = f64::from_bits(0xbfb5_5213_75d1_45cd);
/// The bits of 1 − 2⁻⁴ and 1 + 0x1.09p-4, the bounds of the path near 1.
const NEAR_LO: u64 = 0x3fee_0000_0000_0000;
const NEAR_HI: u64 = 0x3ff1_0900_0000_0000;
/// The bits of 0x1.6p-1, where z's range starts: x's bits less these hold k from bit 52 and the subinterval below it.
const OFF: u64 = 0x3fe6_0000_0000_0000;
/// The bits of the smallest normal number and of +∞, the main path's bounds.
const MIN_NORMAL: u64 = 0x0010_0000_0000_0000;
const INF: u64 = 0x7ff0_0000_0000_0000;
/// 2⁵², which scales a subnormal x into the normals, and 52·2⁵², which takes it back off the exponent's bits.
const TWO52: f64 = f64::from_bits(0x4330_0000_0000_0000);
const EXP52: u64 = 0x0340_0000_0000_0000;
/// 1.5·2⁵²: an integer k under 2⁵¹ in magnitude added to its bits gives the double 1.5·2⁵² + k.
const SHIFT: f64 = f64::from_bits(0x4338_0000_0000_0000);

/// For each of z's 128 subintervals, 1/c and ln c at a c near its centre: word 2i is 1/c's bits, word 2i + 1 ln c's
/// (Arm's `__log_data.tab`). ln c has trailing zeros, so k·LN2_HI + ln c is exact.
#[rustfmt::skip]
static TAB: [u64; 256] = [
    0x3ff7_34f0_c3e0_de9f, 0xbfd7_cc7f_79e6_9000,
    0x3ff7_1378_6a2c_e91f, 0xbfd7_6fee_c20d_0000,
    0x3ff6_f260_08fa_b5a0, 0xbfd7_13e3_1351_e000,
    0x3ff6_d1a6_1f13_8c7d, 0xbfd6_b85b_3828_7800,
    0x3ff6_b149_0bc5_b4d1, 0xbfd6_5d55_9080_7800,
    0x3ff6_9147_332f_0cba, 0xbfd6_02d0_7618_0000,
    0x3ff6_719f_1822_4223, 0xbfd5_a8ca_8690_9000,
    0x3ff6_524f_99a5_1ed9, 0xbfd5_4f43_5603_5000,
    0x3ff6_3356_aa8f_24c4, 0xbfd4_f637_c36b_4000,
    0x3ff6_14b3_6b9d_dc14, 0xbfd4_9da7_fda8_5000,
    0x3ff5_f664_52c6_5c4c, 0xbfd4_4592_3989_a800,
    0x3ff5_d867_b591_2c4f, 0xbfd3_edf4_39b0_b800,
    0x3ff5_babc_cb5b_90de, 0xbfd3_96ce_448f_7000,
    0x3ff5_9d61_f2d9_1a78, 0xbfd3_401e_17bd_a000,
    0x3ff5_8056_1246_5687, 0xbfd2_e9e2_ef46_8000,
    0x3ff5_6397_cee7_6bd3, 0xbfd2_941b_3830_e000,
    0x3ff5_4725_e2a7_7f93, 0xbfd2_3ec5_8cda_8800,
    0x3ff5_2aff_4206_4583, 0xbfd1_e9e1_2927_9000,
    0x3ff5_0f22_dbb2_bddf, 0xbfd1_956d_2b48_f800,
    0x3ff4_f38f_4734_ded7, 0xbfd1_4167_9ab9_f800,
    0x3ff4_d843_cfde_2840, 0xbfd0_edd0_94ef_9800,
    0x3ff4_bd3e_c078_a3c8, 0xbfd0_9aa5_18db_1000,
    0x3ff4_a27f_c3e0_258a, 0xbfd0_47e6_5263_b800,
    0x3ff4_8805_24d4_8434, 0xbfcf_eb22_4586_f000,
    0x3ff4_6dce_1b19_2d0b, 0xbfcf_474a_7517_b000,
    0x3ff4_53d9_d339_1854, 0xbfce_a444_3d10_3000,
    0x3ff4_3a27_44b4_845a, 0xbfce_020d_44e9_b000,
    0x3ff4_20b5_4115_f8fb, 0xbfcd_60a2_2977_f000,
    0x3ff4_0782_da3e_f4b1, 0xbfcc_c001_0495_9000,
    0x3ff3_ee8f_5d57_fe8f, 0xbfcc_2029_5689_1000,
    0x3ff3_d5d9_a00b_4ce9, 0xbfcb_8117_8d81_1000,
    0x3ff3_bd60_c010_c12b, 0xbfca_e2c9_ccd3_d000,
    0x3ff3_a524_2b75_dab8, 0xbfca_4540_2e12_9000,
    0x3ff3_8d22_cd9f_d002, 0xbfc9_a877_681d_f000,
    0x3ff3_755b_c584_7a1c, 0xbfc9_0c6d_6948_3000,
    0x3ff3_5dce_49ad_36e2, 0xbfc8_7120_a645_c000,
    0x3ff3_4679_984d_d440, 0xbfc7_d68f_b414_3000,
    0x3ff3_2f5c_ceff_cb24, 0xbfc7_3cb8_3c62_7000,
    0x3ff3_1877_75a1_0d49, 0xbfc6_a39a_9b37_6000,
    0x3ff3_01c8_373e_3990, 0xbfc6_0b31_54b7_a000,
    0x3ff2_eb4e_bb95_f841, 0xbfc5_737d_7624_3000,
    0x3ff2_d50a_0219_a9d1, 0xbfc4_dc7b_8fc2_3000,
    0x3ff2_bef9_a8b7_fd2a, 0xbfc4_462c_51d2_0000,
    0x3ff2_a91c_7a0c_1bab, 0xbfc3_b08a_bc83_0000,
    0x3ff2_9372_6014_b530, 0xbfc3_1b99_6b49_0000,
    0x3ff2_7dfa_5757_a1f5, 0xbfc2_8754_90a4_4000,
    0x3ff2_68b3_9b1d_3bbf, 0xbfc1_f3b9_f879_a000,
    0x3ff2_539d_838f_f5bd, 0xbfc1_60c8_252c_a000,
    0x3ff2_3eb7_aac9_083b, 0xbfc0_ce7f_57f7_2000,
    0x3ff2_2a01_2ba9_40b6, 0xbfc0_3cdc_49fe_a000,
    0x3ff2_1579_96cc_4132, 0xbfbf_57bd_bc4b_8000,
    0x3ff2_0120_1dd2_fc9b, 0xbfbe_3708_9640_4000,
    0x3ff1_ecf4_494d_480b, 0xbfbd_1798_3ef9_4000,
    0x3ff1_d8f5_528f_6569, 0xbfbb_f967_4ed8_a000,
    0x3ff1_c523_1157_7e7c, 0xbfba_dc79_202f_6000,
    0x3ff1_b17c_74cb_26e9, 0xbfb9_c0c3_e728_8000,
    0x3ff1_9e01_0c2c_1ab6, 0xbfb8_a646_b372_c000,
    0x3ff1_8ab0_7bb6_70bd, 0xbfb7_8d01_b3ac_0000,
    0x3ff1_778a_25ef_bcb6, 0xbfb6_74f1_4538_0000,
    0x3ff1_648d_354c_31da, 0xbfb5_5e0e_6d87_8000,
    0x3ff1_51b9_9027_5fdd, 0xbfb4_485c_dea1_e000,
    0x3ff1_3f0e_a432_d24c, 0xbfb3_33d9_4d6a_a000,
    0x3ff1_2c8b_7210_f9da, 0xbfb2_2079_f8c5_6000,
    0x3ff1_1a30_28ec_b531, 0xbfb1_0e46_9862_2000,
    0x3ff1_07fb_da84_34af, 0xbfaf_fa6c_6ad2_0000,
    0x3ff0_f5ee_0f4e_6bb3, 0xbfad_da8d_4a77_4000,
    0x3ff0_e406_5d2a_9fce, 0xbfab_bcec_e485_0000,
    0x3ff0_d244_632c_a521, 0xbfa9_a189_4012_c000,
    0x3ff0_c0a7_7ce2_981a, 0xbfa7_8858_3302_c000,
    0x3ff0_af2f_83c6_36d1, 0xbfa5_715e_67d6_8000,
    0x3ff0_9ddb_98a0_1339, 0xbfa3_5c8a_4965_8000,
    0x3ff0_8cab_af52_e7df, 0xbfa1_49e3_6415_4000,
    0x3ff0_7b9f_2f4e_28fb, 0xbf9e_72c0_82eb_8000,
    0x3ff0_6ab5_8c35_8f19, 0xbf9a_55f1_5252_8000,
    0x3ff0_59ee_a5ec_f92c, 0xbf96_3d62_cf81_8000,
    0x3ff0_4949_cdd1_2c90, 0xbf92_28fb_8caa_0000,
    0x3ff0_38c6_c6f0_ada9, 0xbf8c_317b_20f9_0000,
    0x3ff0_2865_1379_32a9, 0xbf84_1935_5daa_0000,
    0x3ff0_1824_27ea_7348, 0xbf78_1203_c2ec_0000,
    0x3ff0_0804_0614_b195, 0xbf60_0409_7924_0000,
    0x3fef_e01f_f726_fa1a, 0x3f6f_eff3_8490_0000,
    0x3fef_a11c_c261_ea74, 0x3f87_dc41_353d_0000,
    0x3fef_6310_b081_992e, 0x3f93_cea3_c4c2_8000,
    0x3fef_25f6_3cee_adcd, 0x3f9b_9fc1_1489_0000,
    0x3fee_e9c8_0391_13e7, 0x3fa1_b0d8_ce11_0000,
    0x3fee_ae80_78cb_b1ab, 0x3fa5_8a5b_d001_c000,
    0x3fee_741a_a29d_0c9b, 0x3fa9_5c83_40d8_8000,
    0x3fee_3a91_830a_99b5, 0x3fad_276a_ef57_8000,
    0x3fee_01e0_0960_9a56, 0x3fb0_7598_e598_c000,
    0x3fed_ca01_e577_bb98, 0x3fb2_53f5_e30d_2000,
    0x3fed_92f2_0b7c_9103, 0x3fb4_2edd_8b38_0000,
    0x3fed_5cac_66fb_5cce, 0x3fb6_0659_8757_c000,
    0x3fed_272c_aa5e_de9d, 0x3fb7_da76_356a_0000,
    0x3fec_f26e_3e6b_2ccd, 0x3fb9_ab43_4e1c_6000,
    0x3fec_be6d_a2a7_7902, 0x3fbb_78c7_bb0d_6000,
    0x3fec_8b26_6d37_086d, 0x3fbd_4313_32e7_2000,
    0x3fec_5894_bd5d_5804, 0x3fbf_0a31_71de_6000,
    0x3fec_26b5_33bb_9f8c, 0x3fc0_6715_2b91_4000,
    0x3feb_f583_eeec_e73f, 0x3fc1_4785_8292_b000,
    0x3feb_c4fd_75db_96c1, 0x3fc2_266e_cdca_3000,
    0x3feb_951e_0c86_4a28, 0x3fc3_03d7_a6c5_5000,
    0x3feb_65e2_c5ef_3e2c, 0x3fc3_dfc3_3c33_1000,
    0x3feb_3748_67c9_888b, 0x3fc4_ba36_6b7a_8000,
    0x3feb_094b_211d_304a, 0x3fc5_9339_28d1_f000,
    0x3fea_dbe8_85f2_ef7e, 0x3fc6_6acd_2418_f000,
    0x3fea_af1d_3160_3da2, 0x3fc7_40f8_ec66_9000,
    0x3fea_82e6_3fd3_58a7, 0x3fc8_15c0_f51a_f000,
    0x3fea_5740_ef09_738b, 0x3fc8_e929_54f6_8000,
    0x3fea_2c2a_90ab_4b27, 0x3fc9_bb36_02f8_4000,
    0x3fea_01a0_1393_f2d1, 0x3fca_8bed_1c2c_0000,
    0x3fe9_d79f_24db_3c1b, 0x3fcb_5b51_5c01_d000,
    0x3fe9_ae25_05c7_b190, 0x3fcc_2967_ccbc_c000,
    0x3fe9_852e_f297_ce2f, 0x3fcc_f635_d548_6000,
    0x3fe9_5cba_eea4_4b75, 0x3fcd_c1bd_3446_c000,
    0x3fe9_34c6_9de7_4838, 0x3fce_8c01_b8cf_e000,
    0x3fe9_0d4f_2f67_52e6, 0x3fcf_5509_c017_9000,
    0x3fe8_e652_8eff_d79d, 0x3fd0_0e6c_121f_b800,
    0x3fe8_bfce_9fcc_007c, 0x3fd0_71b8_0e93_d000,
    0x3fe8_99c0_dabe_c30e, 0x3fd0_d46b_9e86_7000,
    0x3fe8_7427_aa23_17fb, 0x3fd1_3687_334b_d000,
    0x3fe8_4f00_acb3_9a08, 0x3fd1_980d_6723_4800,
    0x3fe8_2a49_e865_3e55, 0x3fd1_f8ff_e0cc_8000,
    0x3fe8_0601_95f4_0260, 0x3fd2_595f_d763_6800,
    0x3fe7_e225_63e0_a329, 0x3fd2_b930_0914_a800,
    0x3fe7_beb3_77dc_b5ad, 0x3fd3_1872_1043_6000,
    0x3fe7_9baa_6797_25c2, 0x3fd3_7726_6dec_1800,
    0x3fe7_7907_f217_0657, 0x3fd3_d54f_fbaf_3000,
    0x3fe7_56ca_dbd6_130c, 0x3fd4_32ee_e32f_e000,
];

/// c = chi + clo for each subinterval: word 2i is chi's bits, word 2i + 1 clo's (Arm's `__log_data.tab2`, which
/// stands in for FMA: z − chi − clo is exact to within 2⁻⁶⁶). The `fast` feature's FMA needs none.
#[cfg(not(feature = "fast"))]
#[rustfmt::skip]
static TAB2: [u64; 256] = [
    0x3fe6_1000_014f_b66b, 0x3c7e_026c_9142_5b3c,
    0x3fe6_3000_034d_b495, 0x3c8d_bfea_4800_5d41,
    0x3fe6_5000_0d94_d478, 0x3c8e_7fa7_86d6_a5b7,
    0x3fe6_7000_074e_6fad, 0x3c61_fcea_6b54_254c,
    0x3fe6_8fff_fedf_0fae, 0xbc7c_7e27_4c59_0efd,
    0x3fe6_b000_0763_c5bc, 0xbc8a_c168_48dc_da01,
    0x3fe6_d000_1e5c_c1f6, 0x3c83_3f1c_9d49_9311,
    0x3fe6_efff_eb05_f63e, 0xbc7e_8004_1ae2_2d53,
    0x3fe7_1000_0e86_9780, 0x3c7b_ff66_7109_7952,
    0x3fe7_2fff_fc67_e912, 0x3c8c_00e2_26bd_8724,
    0x3fe7_4fff_df81_116a, 0xbc6e_0291_6ef1_01d2,
    0x3fe7_7000_0f67_9c90, 0xbc67_fc71_cd54_9c74,
    0x3fe7_8fff_fa7e_c835, 0x3c81_bec1_9ef5_0483,
    0x3fe7_afff_fe20_c2e6, 0xbc70_7e17_29cc_6465,
    0x3fe7_cfff_ed3f_c900, 0xbc80_8072_087b_8b1c,
    0x3fe7_efff_e926_1a76, 0x3c8d_c028_6d9d_f9ae,
    0x3fe8_1000_049c_a3e8, 0x3c89_7fd2_51e5_4c33,
    0x3fe8_3000_1793_2c8f, 0xbc8a_fee9_b630_f381,
    0x3fe8_5000_0633_739c, 0x3c89_bfbf_6b65_35bc,
    0x3fe8_7000_2042_89c6, 0xbc8b_bf65_f311_7b75,
    0x3fe8_8fff_ebf5_7904, 0xbc89_006e_a23d_cb57,
    0x3fe8_b000_22bc_04df, 0xbc7d_00df_38e0_4b0a,
    0x3fe8_cfff_e50c_1b8a, 0xbc88_0071_46ff_9f05,
    0x3fe8_efff_fc91_8e43, 0x3c83_817b_d07a_7038,
    0x3fe9_1000_1efa_5fc7, 0x3c89_3e91_76df_b403,
    0x3fe9_3000_1346_7bb9, 0x3c7f_804e_4b98_0276,
    0x3fe9_4fff_e6ee_076f, 0xbc8f_7ef0_d9ff_622e,
    0x3fe9_6fff_de3c_12d1, 0xbc70_82aa_9626_38ba,
    0x3fe9_8fff_f445_8a0d, 0xbc87_801b_9164_a8ef,
    0x3fe9_afff_dd98_2e3e, 0xbc87_40e0_8a5a_9337,
    0x3fe9_cfff_ed49_fb66, 0x3c3f_ce08_c19b_e000,
    0x3fe9_f000_20f1_9c51, 0xbc8a_3faa_2788_5b0a,
    0x3fea_1000_1145_b006, 0x3c74_ff48_9958_da56,
    0x3fea_3000_07bb_f6fa, 0x3c8c_beab_8a2b_6d18,
    0x3fea_5000_1097_1d79, 0x3c88_feca_dd78_7930,
    0x3fea_7000_1df5_2e48, 0xbc8f_4176_3dd8_abdb,
    0x3fea_9000_1c59_3352, 0xbc8e_bf02_84c2_7612,
    0x3fea_b000_2a4f_3e4b, 0xbc69_fd04_3cff_3f5f,
    0x3fea_cfff_d7ae_1ed1, 0xbc82_3ee7_1290_70b4,
    0x3fea_efff_ee51_0478, 0x3c6a_063e_e00e_dea3,
    0x3feb_0fff_db65_0d5b, 0x3c5a_06c8_381f_0ab9,
    0x3feb_2fff_feaa_ca57, 0xbc79_011e_7423_3c1d,
    0x3feb_4fff_d995_badc, 0xbc79_ff10_6886_2a9f,
    0x3feb_7000_249e_659c, 0x3c8a_ff45_d086_4f3e,
    0x3feb_8fff_f987_1640, 0x3c7c_fe77_96c2_c3f9,
    0x3feb_afff_d204_cb4f, 0xbc63_ff27_eef2_2bc4,
    0x3feb_cfff_d241_5c45, 0xbc6c_ffb7_ee3b_ea21,
    0x3feb_efff_f863_09df, 0xbc81_4103_972e_0b5c,
    0x3fec_0fff_e1b5_7653, 0x3c8b_c164_94b7_6a19,
    0x3fec_2fff_f1fa_57e3, 0xbc64_feef_8d30_c6ed,
    0x3fec_4fff_dcbf_e424, 0xbc84_3f68_bcec_4775,
    0x3fec_6fff_ed54_b9f7, 0x3c84_7ea3_f053_e0ec,
    0x3fec_8fff_eb99_8fd5, 0x3c73_8306_8df9_92f1,
    0x3fec_b000_2125_219a, 0xbc68_fd8e_6418_0e04,
    0x3fec_cfff_dd94_469c, 0x3c8e_7ebe_1cc7_ea72,
    0x3fec_efff_eafd_c476, 0x3c8e_be39_ad9f_88fe,
    0x3fed_1000_169a_f82b, 0x3c75_7d91_a8b9_5a71,
    0x3fed_3000_0d0f_f71d, 0x3c89_c190_6970_c7da,
    0x3fed_4fff_ea79_0fc4, 0xbc58_0e37_c558_fe0c,
    0x3fed_7000_2edc_87e5, 0xbc7f_80d6_4dc1_0f44,
    0x3fed_9000_21dc_82aa, 0xbc74_7c8f_94fd_5c5c,
    0x3fed_afff_d86b_0283, 0x3c8c_7f1d_c521_617e,
    0x3fed_d000_296c_4739, 0x3c88_019e_b2ff_b153,
    0x3fed_efff_e544_90f5, 0x3c6e_00d2_c652_cc89,
    0x3fee_0fff_cdab_f694, 0xbc7f_8340_202d_69d2,
    0x3fee_2fff_db52_c8dd, 0x3c7b_00c1_ca1b_0864,
    0x3fee_4fff_f242_16ef, 0x3c72_ffa8_b094_ab51,
    0x3fee_6fff_e88a_5e11, 0xbc57_f673_b1ef_be59,
    0x3fee_9000_119e_ff0d, 0xbc84_808d_5e0b_c801,
    0x3fee_afff_dfa5_1744, 0x3c78_0006_d543_20b5,
    0x3fee_d000_1a12_7fa1, 0xbc50_02f8_6056_5c92,
    0x3fee_f000_07ba_bcc4, 0xbc85_4044_5d35_e611,
    0x3fef_0fff_f57a_8d02, 0xbc4f_fb31_39ef_9105,
    0x3fef_3000_1ee5_8ac7, 0x3c8a_81ac_f273_1155,
    0x3fef_4fff_f582_3494, 0x3c8a_3f41_d4d7_c743,
    0x3fef_6fff_fca9_4c6b, 0xbc62_02f4_1c98_7875,
    0x3fef_8fff_e1f9_c441, 0x3c77_7dd1_f477_e74b,
    0x3fef_afff_d2e0_e37e, 0xbc6f_0119_9a7c_a331,
    0x3fef_d000_1c77_e49e, 0x3c71_81ee_4bce_acb1,
    0x3fef_efff_f7e0_c331, 0xbc6e_0537_0170_875a,
    0x3ff0_0fff_f465_606e, 0xbc8a_7ead_491c_0ada,
    0x3ff0_2fff_f386_7a58, 0xbc97_7f69_c3fc_b2e0,
    0x3ff0_4fff_fdfc_0d17, 0x3c97_bffe_34cb_945b,
    0x3ff0_7000_03cd_4d82, 0x3c82_0083_c0e4_56cb,
    0x3ff0_8fff_f9f2_cbe8, 0xbc6d_ffdf_be37_751a,
    0x3ff0_b000_010c_da65, 0xbc91_3f7f_aee6_26eb,
    0x3ff0_d000_01a4_d338, 0x3c80_7dfa_7948_9ff7,
    0x3ff0_efff_fada_fdfd, 0xbc77_0405_70d6_6bc0,
    0x3ff1_1000_0bba_fd96, 0x3c8e_80d4_846d_0b62,
    0x3ff1_2fff_fae5_f45d, 0x3c9d_bffa_64fd_36ef,
    0x3ff1_5000_0dd5_9ad9, 0x3c9a_0077_7012_50ae,
    0x3ff1_7000_0f21_559a, 0x3c8d_fdf9_e2e3_deee,
    0x3ff1_8fff_fc27_5426, 0x3c91_0030_dc3b_7273,
    0x3ff1_b000_123d_3c59, 0x3c99_7f79_8003_0188,
    0x3ff1_cfff_f829_9eb7, 0xbc65_f932_ab9f_8c67,
    0x3ff1_efff_f48a_d400, 0x3c93_7fbf_9da7_5beb,
    0x3ff2_1000_0c8b_86a4, 0x3c9f_806b_91fd_5b22,
    0x3ff2_3000_0385_4303, 0x3c93_ffc2_eb9f_bf33,
    0x3ff2_4fff_ffbc_f684, 0x3c76_01e7_7e2e_2e72,
    0x3ff2_6fff_f529_21d9, 0x3c7f_fcbb_767f_0c61,
    0x3ff2_9000_1493_3a3c, 0xbc72_02ca_3c02_412b,
    0x3ff2_b000_1455_6313, 0xbc92_8082_33f2_1f02,
    0x3ff2_cfff_ebfe_523b, 0xbc88_ff7e_384f_dcf2,
    0x3ff2_f000_0bb8_ad96, 0xbc85_ff51_5030_41c5,
    0x3ff3_0fff_fb7a_e2af, 0xbc81_0071_885e_289d,
    0x3ff3_2fff_feac_5f7f, 0xbc91_ff5d_3fb7_b715,
    0x3ff3_5000_0ca6_6756, 0x3c95_7f82_228b_82bd,
    0x3ff3_7000_11fb_f721, 0x3c80_00ba_c40d_d5cc,
    0x3ff3_8fff_f959_2fb9, 0xbc94_3f9d_2db2_a751,
    0x3ff3_b000_04dd_d242, 0x3c85_7f6b_7076_38e1,
    0x3ff3_cfff_f5b2_c957, 0x3c7a_023a_10bf_1231,
    0x3ff3_efff_eab0_b418, 0x3c98_7f6d_66b1_52b0,
    0x3ff4_1000_1532_aff4, 0x3c67_f837_5f19_8524,
    0x3ff4_3000_1747_8b29, 0x3c83_01e6_72dc_5143,
    0x3ff4_4fff_e795_b463, 0x3c89_ff69_b8b2_895a,
    0x3ff4_6fff_e804_75e0, 0xbc95_c0b1_9bc2_f254,
    0x3ff4_8fff_ef6f_c1e7, 0x3c9b_4009_f23a_2a72,
    0x3ff4_afff_e5be_a704, 0xbc94_ffb7_bf0d_7d45,
    0x3ff4_d000_1710_27de, 0xbc99_c064_71dc_6a3d,
    0x3ff4_f000_0ff0_3ee2, 0x3c97_7f89_0b85_531c,
    0x3ff5_1000_12dc_4bd1, 0x3c60_0465_7166_a436,
    0x3ff5_3000_1605_277a, 0xbc96_bfce_ce23_3209,
    0x3ff5_4fff_ecdb_704c, 0xbc89_0272_0505_a1d7,
    0x3ff5_6fff_ef5f_54a9, 0x3c9b_bfe6_0ec9_6412,
    0x3ff5_9000_17e6_1012, 0x3c88_7ec5_81af_ef90,
    0x3ff5_b000_03c9_3e92, 0xbc9f_4108_0abf_0cc0,
    0x3ff5_d000_1d49_19bc, 0xbc98_812a_fb25_4729,
    0x3ff5_efff_e7b8_7a89, 0xbc94_7eb7_80ed_6904,
];

/// Word `i` of `table`, `i` masked to its length: the lookup cannot miss, and the compiler drops the check.
#[inline(always)]
fn word(table: &[u64; 256], i: u64) -> u64 {
    table.get((i & 0xff) as usize).copied().unwrap_or(0)
}

/// r = z/c − 1, rounded about once: from the second table, (z − chi − clo)·(1/c), or with the `fast` feature by FMA,
/// Arm's form (PLAN.md M23.7).
#[inline(always)]
fn ratio(z: f64, invc: f64, i: u64) -> f64 {
    #[cfg(not(feature = "fast"))]
    return (z - f64::from_bits(word(&TAB2, i)) - f64::from_bits(word(&TAB2, i.wrapping_add(1)))) * invc;
    #[cfg(feature = "fast")]
    {
        let _ = i;
        crate::num::fused::ratio(z, invc)
    }
}

/// The main path at the bits of a positive normal x, or of a subnormal one normalised: k·ln 2 + ln c + ln(1 + r).
#[inline(always)]
fn reduced(ix: u64) -> f64 {
    let tmp = ix.wrapping_sub(OFF);
    let i = ((tmp >> 45) & 0x7f).wrapping_shl(1);
    let k = (tmp.cast_signed() >> 52).cast_unsigned();
    let z = f64::from_bits(ix.wrapping_sub(tmp & 0xfff0_0000_0000_0000));
    let (invc, logc) = (f64::from_bits(word(&TAB, i)), f64::from_bits(word(&TAB, i.wrapping_add(1))));
    let r = ratio(z, invc, i);
    let kd = f64::from_bits(SHIFT.to_bits().wrapping_add(k)) - SHIFT;
    let (hi, lo) = fast_two_sum(kd * LN2_HI + logc, r);
    let lo = lo + kd * LN2_LO;
    let r2 = r * r;
    lo + r2 * A0 + r * r2 * (A1 + r * A2 + r2 * (A3 + r * A4)) + hi
}

/// ln x near 1, from r = x − 1, which is exact: r − r²/2 summed exactly in hi + lo, r split at 2²⁷ so that its
/// square's halves are exact, and the rest a polynomial in r.
#[inline(always)]
fn near_one(x: f64) -> f64 {
    let r = x - 1.0;
    let r2 = r * r;
    let r3 = r * r2;
    let y = r3 * (B1 + r * B2 + r2 * B3 + r3 * (B4 + r * B5 + r2 * B6 + r3 * (B7 + r * B8 + r2 * B9 + r3 * B10)));
    let (rhi, rlo) = split(r);
    let (hi, lo) = fast_two_sum(r, rhi * rhi * B0);
    let lo = lo + B0 * rlo * (rhi + r);
    y + lo + hi
}

/// Whether `x` takes the path near 1: 1 − 2⁻⁴ ≤ x < 1 + 0x1.09p-4.
#[inline(always)]
fn near(x: f64) -> bool {
    (NEAR_LO..NEAR_HI).contains(&x.to_bits())
}

/// Whether `x` takes the main path: a positive normal number, not near 1.
#[inline(always)]
fn plain(x: f64) -> bool {
    // Equivalent (M23.3): on the main path `ln_any` is `reduced`'s value bit for bit, so taking it at every `x`
    // changes only the speed.
    // equivalent mutant: replace plain -> bool with false
    (MIN_NORMAL..INF).contains(&x.to_bits()) && !near(x)
}

/// ln at any `x`: the main path, with a subnormal x scaled into the normals, the path near 1, and the special cases,
/// −∞ at ±0, NaN below 0, x itself at +∞ and NaN; each is computed and selected. On the main path it is
/// [`reduced`]'s value bit for bit, and near 1 [`near_one`]'s.
#[inline(always)]
fn ln_any(x: f64) -> f64 {
    let ix = x.to_bits();
    let scaled = (x * TWO52).to_bits().wrapping_sub(EXP52);
    // Equivalent (M23.3): at the smallest normal x, x·2⁵² with 52 taken from its exponent is x's own bits.
    // equivalent mutant: replace < with <=
    let r = reduced(if ix < MIN_NORMAL { scaled } else { ix });
    let r = if near(x) { near_one(x) } else { r };
    let r = if ix >= INF { f64::NAN } else { r };
    let r = if ix == INF || x.is_nan() { x } else { r };
    if ix << 1 == 0 { f64::NEG_INFINITY } else { r }
}

/// The natural logarithm, under 1 ulp: Arm's `log` without FMA (PLAN.md M23.3).
pub fn ln(x: f64) -> f64 {
    if plain(x) {
        reduced(x.to_bits())
    } else if near(x) {
        near_one(x)
    } else {
        ln_any(x)
    }
}

/// [`ln`] on every lane: the main path where every lane takes it, else every case on every lane.
#[cfg(any(test, feature = "libm"))]
#[inline(always)]
pub(crate) fn ln_lanes<const W: usize>(x: Lanes<W>) -> Lanes<W> {
    if x.0.iter().all(|&x| plain(x)) { Lanes(x.0.map(|x| reduced(x.to_bits()))) } else { Lanes(x.0.map(ln_any)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::{LaneKernel, run_lanes};
    use crate::num::lanes::tests::{Rng, canonical};

    /// [`ln`] as a lane kernel: the scalar function at a point, [`ln_lanes`] across lanes.
    struct Ln;

    impl LaneKernel for Ln {
        type Out = u64;
        fn point(&self, x: f64, _: f64) -> u64 {
            canonical(ln(x))
        }
        fn lanes<const W: usize>(&self, x: Lanes<W>, _: Lanes<W>) -> [u64; W] {
            ln_lanes(x).0.map(canonical)
        }
    }

    /// The edges of VERIFICATION.md §9.2 for Arm's `log`, each with its neighbours one ulp either side: ±0, negative
    /// inputs (−1, −`f64::MIN_POSITIVE`, −∞), the subnormals and the smallest normal (scaled below it), 1 and the
    /// bounds of the path near 1, z's range 0x1.6p-1 to 0x1.6p0, `f64::MAX`, +∞ and NaN.
    fn edges() -> Vec<f64> {
        let bits = [NEAR_LO, NEAR_HI, OFF, OFF + (1 << 52)];
        let mut edges = vec![0.0, -0.0, 1.0, -1.0, f64::MIN_POSITIVE, -f64::MIN_POSITIVE, 5e-324, 1e-310, 2.0, 0.5];
        edges.extend(bits.map(f64::from_bits));
        edges.extend([f64::MAX, f64::INFINITY, f64::NEG_INFINITY, f64::NAN]);
        edges.iter().flat_map(|&e| [e, e.next_up(), e.next_down()]).collect()
    }

    /// 30 000 inputs each over τ's range in α^r (10⁻³ to 20, whole groups on the main path), over [0.9, 1.1] (groups
    /// that mix the main path with the path near 1), over [0, 10⁶] and over every bit pattern; then each edge alone
    /// among seven ordinary lanes, and the edges side by side.
    fn inputs() -> Vec<f64> {
        let mut rng = Rng(23);
        let mut x: Vec<f64> = (0..30_000).map(|_| rng.uniform(1e-3, 20.0)).collect();
        x.extend((0..30_000).map(|_| rng.uniform(0.9, 1.1)));
        x.extend((0..30_000).map(|_| rng.uniform(0.0, 1e6)));
        x.extend((0..30_000).map(|_| f64::from_bits(rng.next())));
        for e in edges() {
            x.push(e);
            x.extend((0..7).map(|_| rng.uniform(1e-3, 20.0)));
        }
        x.extend(edges());
        x
    }

    /// `Ln` over `x` in groups of lanes by `run`, against [`ln`] at each point.
    fn assert_ln(x: &[f64], run: impl Fn(&[f64], &mut [u64]), what: &str) {
        let mut got = vec![0; x.len()];
        run(x, &mut got);
        for (x, got) in x.iter().zip(got) {
            assert!(got == canonical(ln(*x)), "{what}: ln({x:e}) = {:e}, scalar {:e}", f64::from_bits(got), ln(*x));
        }
    }

    /// PLAN.md M23.3 (VERIFICATION.md §9.2, class `Exact`): [`ln_lanes`] is [`ln`] bit for bit (NaN canonical), in
    /// every build, on 2, 4 and 8 lanes and with the `simd` feature at every dispatch level of this CPU, on random
    /// inputs over α^r's τ, near 1, over [0, 10⁶] and over all of `f64`, and on the edges, alone and side by side.
    #[test]
    fn lane_ln_is_scalar_bitwise() {
        let x = inputs();
        assert_ln(&x, |x, out| run_lanes::<_, 2>(&Ln, x, x, out), "2 lanes");
        assert_ln(&x, |x, out| run_lanes::<_, 4>(&Ln, x, x, out), "4 lanes");
        assert_ln(&x, |x, out| run_lanes::<_, 8>(&Ln, x, x, out), "8 lanes");
        #[cfg(feature = "simd")]
        for level in crate::exec::tests::levels() {
            assert_ln(&x, |x, out| crate::exec::run_simd(level, &Ln, x, x, out), &format!("{level:?}"));
        }
        assert!(x.len() > 120_000 && x.iter().any(|x| x.is_nan()));
    }
}
