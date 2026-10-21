//! RP-1485's constants, each with its source (ROT-264: v8.0.0 writes four gas constants and two molar masses of dry air
//! as bare literals; map 07 H4). Three of v8.0.0's four R values are its parts' own: the water and air parts carry
//! theirs (IAPWS-95's 8.314371 J/(mol K), Lemmon's 8.31451), so only the mixture's is here.

/// The molar gas constant of the mixture, J/(mol K): CODATA's, as RP-1485 takes it (Herrmann, Kretzschmar and Gatley
/// 2009; ASHRAE's LibHuAirProp guide §1.3), 8.314472.
pub const R: f64 = 8.314_472;

/// The molar mass of dry air, kg/mol: Gatley et al. (2008), as RP-1485 takes it (LibHuAirProp guide §1.3), 28.966
/// g/mol; ε = M_w/M_a.
pub const M_AIR: f64 = 0.028_966;

/// The molar mass of water, kg/mol: IAPWS-95's (LibHuAirProp guide §1.3).
pub const M_WATER: f64 = 0.018_015_268;

/// Dry air's mole fractions of N₂, O₂ and Ar, Lemmon et al. (2000)'s, which RP-1485 takes for Henry's constant
/// (LibHuAirProp guide §1.3).
pub const AIR_FRACTIONS: [f64; 3] = [0.7812, 0.2096, 0.0092];
