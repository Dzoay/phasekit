// What the calculator shows: every output key of the package (`outputKeys()`, CoolProp's 85) but Phase, which the page
// shows as the state's phase name. Each key has a symbol, a label and the kind of quantity its SI value is, after
// CoolProp's parameter table (`DataStructures.cpp:20-112`, reference/CoolProp), whose "-" for the virial coefficients
// the page replaces with their dimensions (B in m³/mol, C in m⁶/mol²).
import type { KindId } from "./units";

export interface Output {
  key: string;
  symbol: string;
  label: string;
  kind: KindId;
}

export interface Group {
  title: string;
  outputs: readonly Output[];
}

const o = (key: string, symbol: string, label: string, kind: KindId): Output => ({ key, symbol, label, kind });

export const GROUPS: readonly Group[] = [
  {
    title: "State",
    outputs: [
      o("T", "T", "Temperature", "temperature"),
      o("P", "p", "Pressure", "pressure"),
      o("Dmass", "ρ", "Density", "massDensity"),
      o("Dmolar", "ρₙ", "Molar density", "molarDensity"),
      o("Q", "q", "Vapour quality", "moleFraction"),
      o("Qmass", "qₘ", "Vapour quality, mass basis", "massFraction"),
      o("Delta", "δ", "Reduced density ρ/ρᵣ", "dimensionless"),
      o("Tau", "τ", "Inverse reduced temperature Tᵣ/T", "dimensionless"),
    ],
  },
  {
    title: "Energy",
    outputs: [
      o("Hmass", "h", "Enthalpy", "specificEnergy"),
      o("Smass", "s", "Entropy", "specificEntropy"),
      o("Umass", "u", "Internal energy", "specificEnergy"),
      o("Gmass", "g", "Gibbs energy", "specificEnergy"),
      o("Helmholtzmass", "a", "Helmholtz energy", "specificEnergy"),
      o("Hmolar", "hₙ", "Molar enthalpy", "molarEnergy"),
      o("Smolar", "sₙ", "Molar entropy", "molarEntropy"),
      o("Umolar", "uₙ", "Molar internal energy", "molarEnergy"),
      o("Gmolar", "gₙ", "Molar Gibbs energy", "molarEnergy"),
      o("Helmholtzmolar", "aₙ", "Molar Helmholtz energy", "molarEnergy"),
    ],
  },
  {
    title: "Heat capacities",
    outputs: [
      o("Cpmass", "c_p", "Isobaric heat capacity", "specificEntropy"),
      o("Cvmass", "c_v", "Isochoric heat capacity", "specificEntropy"),
      o("Cp0mass", "c_p⁰", "Ideal-gas isobaric heat capacity", "specificEntropy"),
      o("Cpmolar", "c_p,ₙ", "Molar isobaric heat capacity", "molarEntropy"),
      o("Cvmolar", "c_v,ₙ", "Molar isochoric heat capacity", "molarEntropy"),
      o("Cp0molar", "c_p,ₙ⁰", "Molar ideal-gas isobaric heat capacity", "molarEntropy"),
    ],
  },
  {
    title: "Transport and interface",
    outputs: [
      o("viscosity", "η", "Viscosity", "viscosity"),
      o("conductivity", "λ", "Thermal conductivity", "conductivity"),
      o("Prandtl", "Pr", "Prandtl number", "dimensionless"),
      o("surface_tension", "σ", "Surface tension", "surfaceTension"),
    ],
  },
  {
    title: "Derivatives and ratios",
    outputs: [
      o("speed_of_sound", "w", "Speed of sound", "speed"),
      o("Z", "Z", "Compressibility factor", "dimensionless"),
      o("isothermal_compressibility", "κ_T", "Isothermal compressibility", "inversePressure"),
      o("isobaric_expansion_coefficient", "β", "Isobaric expansion coefficient", "inverseTemperature"),
      o("isentropic_expansion_coefficient", "κ_s", "Isentropic expansion coefficient", "dimensionless"),
      o("fundamental_derivative_of_gas_dynamics", "Γ", "Fundamental derivative of gas dynamics", "dimensionless"),
      o("PIP", "Π", "Phase identification parameter", "dimensionless"),
    ],
  },
  {
    title: "Ideal gas and residual parts",
    outputs: [
      o("Hmass_idealgas", "h⁰", "Ideal-gas enthalpy", "specificEnergy"),
      o("Smass_idealgas", "s⁰", "Ideal-gas entropy", "specificEntropy"),
      o("Umass_idealgas", "u⁰", "Ideal-gas internal energy", "specificEnergy"),
      o("Hmolar_idealgas", "hₙ⁰", "Molar ideal-gas enthalpy", "molarEnergy"),
      o("Smolar_idealgas", "sₙ⁰", "Molar ideal-gas entropy", "molarEntropy"),
      o("Umolar_idealgas", "uₙ⁰", "Molar ideal-gas internal energy", "molarEnergy"),
      o("Hmolar_residual", "hₙʳ", "Molar residual enthalpy", "molarEnergy"),
      o("Smolar_residual", "sₙʳ", "Molar residual entropy", "molarEntropy"),
      o("Gmolar_residual", "gₙʳ", "Molar residual Gibbs energy", "molarEnergy"),
    ],
  },
  {
    title: "Helmholtz energy",
    outputs: [
      o("alphar", "αʳ", "Residual Helmholtz energy", "dimensionless"),
      o("dalphar_dtau_constdelta", "∂αʳ/∂τ", "Its τ derivative at constant δ", "dimensionless"),
      o("dalphar_ddelta_consttau", "∂αʳ/∂δ", "Its δ derivative at constant τ", "dimensionless"),
      o("alpha0", "α⁰", "Ideal-gas Helmholtz energy", "dimensionless"),
      o("dalpha0_dtau_constdelta", "∂α⁰/∂τ", "Its τ derivative at constant δ", "dimensionless"),
      o("dalpha0_ddelta_consttau", "∂α⁰/∂δ", "Its δ derivative at constant τ", "dimensionless"),
      o("d2alpha0_ddelta2_consttau", "∂²α⁰/∂δ²", "Its second δ derivative", "dimensionless"),
      o("d3alpha0_ddelta3_consttau", "∂³α⁰/∂δ³", "Its third δ derivative", "dimensionless"),
    ],
  },
  {
    title: "Virial coefficients",
    outputs: [
      o("Bvirial", "B", "Second virial coefficient", "molarVolume"),
      o("Cvirial", "C", "Third virial coefficient", "molarVolumeSquared"),
      o("dBvirial_dT", "dB/dT", "Its temperature derivative", "molarVolumePerKelvin"),
      o("dCvirial_dT", "dC/dT", "Its temperature derivative", "molarVolumeSquaredPerKelvin"),
    ],
  },
  {
    title: "Fluid constants",
    outputs: [
      o("molar_mass", "M", "Molar mass", "molarMass"),
      o("gas_constant", "R", "Molar gas constant", "molarEntropy"),
      o("acentric", "ω", "Acentric factor", "dimensionless"),
      o("dipole_moment", "μ", "Dipole moment", "dipoleMoment"),
      o("T_critical", "T_c", "Critical temperature", "temperature"),
      o("p_critical", "p_c", "Critical pressure", "pressure"),
      o("rhomass_critical", "ρ_c", "Critical density", "massDensity"),
      o("rhomolar_critical", "ρₙ,c", "Critical molar density", "molarDensity"),
      o("T_reducing", "T_r", "Reducing temperature", "temperature"),
      o("p_reducing", "p_r", "Reducing pressure", "pressure"),
      o("rhomass_reducing", "ρ_r", "Reducing density", "massDensity"),
      o("rhomolar_reducing", "ρₙ,r", "Reducing molar density", "molarDensity"),
      o("T_triple", "T_t", "Triple-point temperature", "temperature"),
      o("p_triple", "p_t", "Triple-point pressure", "pressure"),
      o("T_min", "T_min", "Lowest temperature of the equation of state", "temperature"),
      o("T_max", "T_max", "Highest temperature of the equation of state", "temperature"),
      o("P_min", "p_min", "Lowest pressure", "pressure"),
      o("P_max", "p_max", "Highest pressure of the equation of state", "pressure"),
      o("T_freeze", "T_f", "Freezing temperature (incompressibles)", "temperature"),
      o("fraction_min", "x_min", "Lowest fraction (incompressibles)", "dimensionless"),
      o("fraction_max", "x_max", "Highest fraction (incompressibles)", "dimensionless"),
    ],
  },
  {
    title: "Environment and safety",
    outputs: [
      o("GWP20", "GWP₂₀", "Global warming potential, 20 years", "dimensionless"),
      o("GWP100", "GWP₁₀₀", "Global warming potential, 100 years", "dimensionless"),
      o("GWP500", "GWP₅₀₀", "Global warming potential, 500 years", "dimensionless"),
      o("ODP", "ODP", "Ozone depletion potential", "dimensionless"),
      o("FH", "FH", "Flammability hazard (NFPA 704)", "dimensionless"),
      o("HH", "HH", "Health hazard (NFPA 704)", "dimensionless"),
      o("PH", "PH", "Physical hazard (NFPA 704)", "dimensionless"),
    ],
  },
];

export const OUTPUTS: readonly Output[] = GROUPS.flatMap((g) => g.outputs);

/** The keys an input field takes, in the order the calculator offers them. */
export const INPUTS: readonly string[] = [
  "T",
  "P",
  "Q",
  "Dmass",
  "Hmass",
  "Smass",
  "Umass",
  "Dmolar",
  "Hmolar",
  "Smolar",
  "Umolar",
];

/** The headline outputs, shown large above the tables. */
export const HEADLINE: readonly string[] = ["T", "P", "Dmass", "Hmass", "Smass", "Q"];

export function output(key: string): Output | undefined {
  return OUTPUTS.find((x) => x.key === key);
}
