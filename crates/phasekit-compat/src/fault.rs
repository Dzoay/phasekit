//! Fault injection for the facades' tests (PLAN.md M24.1; issue #219), behind the `fault-injection` feature, which no
//! build that ships turns on. [`with_faults`] gives a registry a resolver that panics, on purpose, where a call reaches
//! the registry or a model for one of four names, so that a test can check that the panic unwinds through compat and
//! the registry to the facade's catch (`PK_PANIC` at the C boundary, CoolProp's failure value in the shim, the panic
//! hook in the browser package), along the path a real bug would take.

#![allow(clippy::panic, reason = "the panics this module injects on purpose, for the four names alone")]

use std::sync::Arc;

use phasekit_core::{
    Capabilities, Error, FlashOptions, FluidInfo, NativeInput, Registry, Resolver, State, ThermoModel,
};

/// Resolving it panics, inside the registry's resolver.
pub const RESOLVE: &str = "fault::resolve";
/// It resolves to a model whose `info` panics.
pub const INFO: &str = "fault::info";
/// It resolves to a model whose flash panics.
pub const FLASH: &str = "fault::flash";
/// It resolves to a model whose drop panics.
pub const DROP: &str = "fault::drop";

/// `registry`, whose [`Registry::resolve`] also reads the four names: [`RESOLVE`], [`INFO`], [`FLASH`] and [`DROP`].
/// Each model is the registry's Water but for the one call that panics.
pub fn with_faults(registry: &Registry) -> Registry {
    registry.with_resolver(Arc::new(Faults))
}

/// The resolver [`with_faults`] registers.
#[derive(Debug)]
struct Faults;

impl Resolver for Faults {
    fn resolve(&self, name: &str, registry: &Registry) -> Option<Result<Arc<dyn ThermoModel>, Error>> {
        let fault = match name {
            RESOLVE => panic!("fault injection: resolving {RESOLVE}"),
            INFO => Fault::Info,
            FLASH => Fault::Flash,
            DROP => Fault::Drop,
            _ => return None,
        };
        let water = registry.get("Water").cloned();
        Some(water.map(|water| Arc::new(Faulty { water, fault }) as Arc<dyn ThermoModel>))
    }
}

/// The call of a [`Faulty`] model that panics.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Fault {
    Info,
    Flash,
    Drop,
}

/// Water's name, capabilities and flash, but for the call its [`Fault`] names, which panics.
#[derive(Debug)]
struct Faulty {
    water: phasekit_core::Fluid,
    fault: Fault,
}

impl ThermoModel for Faulty {
    fn info(&self) -> &FluidInfo {
        if self.fault == Fault::Info {
            panic!("fault injection: {INFO}'s info");
        }
        self.water.model().info()
    }
    fn capabilities(&self) -> Capabilities {
        self.water.model().capabilities()
    }
    fn flash(&self, input: NativeInput, opts: &FlashOptions) -> Result<State, Error> {
        if self.fault == Fault::Flash {
            panic!("fault injection: {FLASH}'s flash");
        }
        self.water.model().flash(input, opts)
    }
}

impl Drop for Faulty {
    fn drop(&mut self) {
        if self.fault == Fault::Drop && !std::thread::panicking() {
            panic!("fault injection: {DROP}'s drop");
        }
    }
}
