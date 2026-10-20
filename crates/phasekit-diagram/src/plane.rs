//! The five planes (PLAN.md M19.1, the site's `diagrams.ts` of M21.2): their axes, which of them are logarithmic, and
//! the isoline families each draws. At most three families beside quality's, the most a colour-blind-safe palette
//! keeps apart when every pair can sit side by side; no family holds a quantity its plane plots.

use phasekit_core::{Prop, Var};

/// A quantity a plane plots, or a line holds, per unit mass where it has a basis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Quantity {
    /// Temperature, K.
    T,
    /// Pressure, Pa.
    P,
    /// Specific enthalpy, J/kg.
    Hmass,
    /// Specific entropy, J/(kg·K).
    Smass,
    /// Density, kg/m³.
    Dmass,
    /// Vapour quality.
    Q,
}

impl Quantity {
    /// The property the batch outputs for it.
    pub const fn prop(self) -> Prop {
        match self {
            Quantity::T => Prop::T,
            Quantity::P => Prop::P,
            Quantity::Hmass => Prop::Hmass,
            Quantity::Smass => Prop::Smass,
            Quantity::Dmass => Prop::Dmass,
            Quantity::Q => Prop::Q,
        }
    }

    /// The input variable it is, its basis the batch's (mass).
    pub const fn var(self) -> Var {
        match self {
            Quantity::T => Var::T,
            Quantity::P => Var::P,
            Quantity::Hmass => Var::H,
            Quantity::Smass => Var::S,
            Quantity::Dmass => Var::D,
            Quantity::Q => Var::Q,
        }
    }
}

/// An axis: the quantity it plots, and whether its scale is logarithmic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Axis {
    /// The plotted quantity.
    pub quantity: Quantity,
    /// Whether the scale is logarithmic.
    pub log: bool,
}

/// A family of isolines: the quantity each line holds, and the one its sweep runs over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Family {
    /// The quantity each line holds.
    pub held: Quantity,
    /// The quantity the line is swept over.
    pub sweep: Quantity,
}

/// A diagram's plane, named y–x as the diagrams are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Plane {
    /// Pressure against specific enthalpy.
    Ph,
    /// Temperature against specific entropy.
    Ts,
    /// Pressure against temperature.
    Pt,
    /// Temperature against density.
    TRho,
    /// Specific enthalpy against specific entropy.
    Hs,
}

const fn linear(quantity: Quantity) -> Axis {
    Axis { quantity, log: false }
}

const fn log(quantity: Quantity) -> Axis {
    Axis { quantity, log: true }
}

const fn family(held: Quantity, sweep: Quantity) -> Family {
    Family { held, sweep }
}

const QUALITY: Family = family(Quantity::Q, Quantity::T);

const PH: [Family; 4] = [
    family(Quantity::T, Quantity::P),
    family(Quantity::Smass, Quantity::P),
    family(Quantity::Dmass, Quantity::P),
    QUALITY,
];
const TS: [Family; 4] = [
    family(Quantity::P, Quantity::T),
    family(Quantity::Hmass, Quantity::T),
    family(Quantity::Dmass, Quantity::T),
    QUALITY,
];
const PT: [Family; 1] = [family(Quantity::Dmass, Quantity::T)];
const TRHO: [Family; 2] = [family(Quantity::P, Quantity::T), QUALITY];
const HS: [Family; 3] = [family(Quantity::P, Quantity::T), family(Quantity::T, Quantity::P), QUALITY];

impl Plane {
    /// Every plane, in the site's order.
    pub const ALL: [Plane; 5] = [Plane::Ph, Plane::Ts, Plane::Pt, Plane::TRho, Plane::Hs];

    /// Its id, the site's: `ph`, `ts`, `pt`, `trho` or `hs`.
    pub const fn id(self) -> &'static str {
        match self {
            Plane::Ph => "ph",
            Plane::Ts => "ts",
            Plane::Pt => "pt",
            Plane::TRho => "trho",
            Plane::Hs => "hs",
        }
    }

    /// The plane of an id; `None` for any other string.
    pub fn from_id(id: &str) -> Option<Plane> {
        Plane::ALL.into_iter().find(|p| p.id() == id)
    }

    /// The horizontal axis.
    pub const fn x(self) -> Axis {
        match self {
            Plane::Ph => linear(Quantity::Hmass),
            Plane::Ts | Plane::Hs => linear(Quantity::Smass),
            Plane::Pt => linear(Quantity::T),
            Plane::TRho => log(Quantity::Dmass),
        }
    }

    /// The vertical axis.
    pub const fn y(self) -> Axis {
        match self {
            Plane::Ph | Plane::Pt => log(Quantity::P),
            Plane::Ts | Plane::TRho => linear(Quantity::T),
            Plane::Hs => linear(Quantity::Hmass),
        }
    }

    /// The isoline families it draws, quality's last where it has one.
    pub const fn families(self) -> &'static [Family] {
        match self {
            Plane::Ph => &PH,
            Plane::Ts => &TS,
            Plane::Pt => &PT,
            Plane::TRho => &TRHO,
            Plane::Hs => &HS,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The site's `diagrams.ts` (M21.2): five planes, each with at most three coloured families beside quality, none
    /// holding a quantity its plane plots.
    #[test]
    fn five_planes_each_with_at_most_three_families_beside_quality() {
        let ids: Vec<_> = Plane::ALL.iter().map(|p| p.id()).collect();
        assert_eq!(ids, ["ph", "ts", "pt", "trho", "hs"]);
        for plane in Plane::ALL {
            let families = plane.families();
            assert!(families.iter().filter(|f| f.held != Quantity::Q).count() <= 3, "{plane:?}");
            let plotted = [plane.x().quantity, plane.y().quantity];
            assert!(families.iter().all(|f| !plotted.contains(&f.held)), "{plane:?}");
            assert_eq!(Plane::from_id(plane.id()), Some(plane));
        }
        assert_eq!(Plane::from_id("nonsense"), None);
    }

    /// Pressure and density are plotted on logarithmic axes, the rest on linear ones.
    #[test]
    fn pressure_and_density_axes_are_logarithmic() {
        for plane in Plane::ALL {
            for axis in [plane.x(), plane.y()] {
                assert_eq!(axis.log, matches!(axis.quantity, Quantity::P | Quantity::Dmass), "{plane:?}");
            }
        }
        assert_eq!((Plane::Ph.x().quantity, Plane::Ph.y().quantity), (Quantity::Hmass, Quantity::P));
        assert_eq!((Plane::Hs.x().quantity, Plane::Hs.y().quantity), (Quantity::Smass, Quantity::Hmass));
        assert_eq!((Plane::TRho.x().quantity, Plane::TRho.y().quantity), (Quantity::Dmass, Quantity::T));
    }

    /// Each quantity reads its own mass-basis property and is its own input variable.
    #[test]
    fn each_quantity_is_its_property_and_its_variable() {
        let all = [Quantity::T, Quantity::P, Quantity::Hmass, Quantity::Smass, Quantity::Dmass, Quantity::Q];
        let props = [Prop::T, Prop::P, Prop::Hmass, Prop::Smass, Prop::Dmass, Prop::Q];
        let vars = [Var::T, Var::P, Var::H, Var::S, Var::D, Var::Q];
        for ((q, prop), var) in all.into_iter().zip(props).zip(vars) {
            assert_eq!((q.prop(), q.var()), (prop, var));
        }
    }
}
