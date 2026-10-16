// SPDX-License-Identifier: AGPL-3.0-only

//! Shared descriptors for the Rhai command prompt.
//!
//! Every callable application function and every exported constant is declared
//! once, here, together with the code that registers it into a Rhai engine. The
//! engine registration and the completion catalog therefore cannot drift apart:
//! both are derived from the same [`FUNCTIONS`] and [`CONSTANTS`] tables.

use super::host::Host;
use crate::service::Command;
use rhai::{CustomType, Engine, Module};

/// Host-defined unit values for explicit numeric arguments.
///
/// Scripts pass these values instead of strings, for example
/// `seek(90, unit::SEC)`. The values carry no conversion behaviour of their own;
/// checked validation and normalization live with the operation that consumes
/// them.
///
/// The type is opaque to scripts: no field, getter or setter is exposed, so a
/// `Unit` can only be obtained from the `unit` namespace and passed to the
/// registered APIs that accept one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unit {
    kind: UnitKind,
}

impl CustomType for Unit {
    fn build(mut builder: rhai::TypeBuilder<Self>) {
        builder
            .with_name("Unit")
            .on_debug(|unit| format!("unit::{}", unit.label()));
    }
}

/// Discriminated set of [`Unit`] values.
// The variant names mirror the exported script names in [`Unit::label`].
#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnitKind {
    MS,
    SEC,
    MIN,
    HOUR,
    PERCENT,
}

impl Unit {
    /// Milliseconds.
    pub const MS: Self = Self { kind: UnitKind::MS };
    /// Seconds.
    pub const SEC: Self = Self {
        kind: UnitKind::SEC,
    };
    /// Minutes.
    pub const MIN: Self = Self {
        kind: UnitKind::MIN,
    };
    /// Hours.
    pub const HOUR: Self = Self {
        kind: UnitKind::HOUR,
    };
    /// Percentage of the whole track.
    pub const PERCENT: Self = Self {
        kind: UnitKind::PERCENT,
    };

    /// Canonical name exported under the `unit` namespace.
    pub const fn label(self) -> &'static str {
        match self.kind {
            UnitKind::MS => "MS",
            UnitKind::SEC => "SEC",
            UnitKind::MIN => "MIN",
            UnitKind::HOUR => "HOUR",
            UnitKind::PERCENT => "PERCENT",
        }
    }

    /// Parse a canonical name back into a value, for completion and display.
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "MS" => Some(Self::MS),
            "SEC" => Some(Self::SEC),
            "MIN" => Some(Self::MIN),
            "HOUR" => Some(Self::HOUR),
            "PERCENT" => Some(Self::PERCENT),
            _ => None,
        }
    }

    /// Whether the value expresses a percentage of the full track.
    pub const fn is_percent(self) -> bool {
        matches!(self.kind, UnitKind::PERCENT)
    }

    /// Every exported value, in catalog order.
    pub const fn all() -> [Self; 5] {
        [Self::MS, Self::SEC, Self::MIN, Self::HOUR, Self::PERCENT]
    }
}

/// Semantic role of a parameter.
///
/// Roles drive completion and argument hints. They do not perform static type
/// checking: Rhai validates argument types at runtime.
// The roles belong to the registration mechanism this checkpoint introduces;
// the first parameterized APIs arrive with the numeric seek functions.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamRole {
    /// Signed numeric value; integers and finite floats are accepted.
    Value,
    /// Host-defined [`Unit`] value, optionally excluding percentages.
    Unit {
        /// Whether `unit::PERCENT` is a valid argument here.
        percent: bool,
    },
    /// Filesystem path passed as a string.
    Path,
    /// Ordinary string with no completion role.
    Text,
    /// Binding context name passed as a string.
    Context,
    /// Canonical key name passed as a string.
    Key,
}

/// One parameter of a registered function.
// The role and help text are read by completion once parameterized APIs exist.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct Param {
    /// Parameter name as shown in signatures.
    pub name: &'static str,
    /// Semantic role used by completion and argument hints.
    pub role: ParamRole,
    /// Short description shown as help text.
    pub docs: &'static str,
}

/// Registers the callable forms of a function into a Rhai engine.
///
/// A descriptor may install several overloads behind one user-facing signature,
/// as long as the catalog entry describes the callable set accurately.
type InstallFn = fn(&mut Engine, &Host);

/// One callable application API.
#[derive(Clone, Copy, Debug)]
pub struct Function {
    /// Registered script name.
    pub name: &'static str,
    /// Ordered parameters, empty for a call without arguments.
    pub params: &'static [Param],
    /// Description of the returned value.
    pub returns: &'static str,
    /// Short description shown as help text.
    pub docs: &'static str,
    /// Registers the callable forms into an engine.
    install: InstallFn,
}

impl Function {
    const fn new(
        name: &'static str,
        params: &'static [Param],
        returns: &'static str,
        docs: &'static str,
        install: InstallFn,
    ) -> Self {
        Self {
            name,
            params,
            returns,
            docs,
            install,
        }
    }

    /// Human-readable signature, for example `seek(value, unit)`.
    pub fn signature(&self) -> String {
        let params = self
            .params
            .iter()
            .map(|param| param.name)
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}({})", self.name, params)
    }

    /// Register the callable forms into `engine`.
    pub fn install(&self, engine: &mut Engine, host: &Host) {
        (self.install)(engine, host);
    }
}

/// One exported constant namespace value.
#[derive(Clone, Copy, Debug)]
pub struct Constant {
    /// Namespace the value is exported under.
    pub namespace: &'static str,
    /// Canonical name inside the namespace.
    pub name: &'static str,
    /// Registered value.
    pub value: Unit,
    /// Short description shown as help text.
    pub docs: &'static str,
}

impl Constant {
    const fn new(
        namespace: &'static str,
        name: &'static str,
        value: Unit,
        docs: &'static str,
    ) -> Self {
        Self {
            namespace,
            name,
            value,
            docs,
        }
    }

    /// Qualified script name, for example `unit::SEC`.
    pub fn qualified_name(&self) -> String {
        format!("{}::{}", self.namespace, self.name)
    }
}

/// Immutable snapshot of every registration.
///
/// Completion and other read-only consumers take a snapshot instead of holding
/// the engine, so no script is executed and no lock is taken while drawing.
#[derive(Clone, Debug)]
pub struct Catalog {
    functions: Vec<Function>,
    constants: Vec<Constant>,
}

impl Catalog {
    /// Every callable application function.
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }

    /// Every exported constant value.
    pub fn constants(&self) -> &[Constant] {
        &self.constants
    }
}

/// Returns the muse version string as exposed by the `version` function.
fn version() -> String {
    format!("muse {}", env!("CARGO_PKG_VERSION"))
}

/// Registers `version` into an engine.
fn version_install(engine: &mut Engine, _host: &Host) {
    engine.register_fn("version", version);
}

fn play_install(engine: &mut Engine, host: &Host) {
    let host = host.clone();
    engine.register_fn("play", move || host.execute(Command::Play));
}

fn pause_install(engine: &mut Engine, host: &Host) {
    let host = host.clone();
    engine.register_fn("pause", move || host.execute(Command::Pause));
}

fn stop_install(engine: &mut Engine, host: &Host) {
    let host = host.clone();
    engine.register_fn("stop", move || host.execute(Command::Stop));
}

/// Every callable application API.
///
/// Each entry carries the code that registers it into an engine, so a callable
/// and its catalog entry are added together.
pub const FUNCTIONS: &[Function] = &[
    Function::new(
        "version",
        &[],
        "string",
        "Return the muse version string.",
        version_install,
    ),
    Function::new(
        "play",
        &[],
        "()",
        "Resume the loaded track after confirmed service execution.",
        play_install,
    ),
    Function::new(
        "pause",
        &[],
        "()",
        "Pause playback after confirmed service execution.",
        pause_install,
    ),
    Function::new(
        "stop",
        &[],
        "()",
        "Stop playback after confirmed service execution.",
        stop_install,
    ),
];

/// Every exported constant namespace value.
pub const CONSTANTS: &[Constant] = &[
    Constant::new("unit", "MS", Unit::MS, "Milliseconds."),
    Constant::new("unit", "SEC", Unit::SEC, "Seconds."),
    Constant::new("unit", "MIN", Unit::MIN, "Minutes."),
    Constant::new("unit", "HOUR", Unit::HOUR, "Hours."),
    Constant::new(
        "unit",
        "PERCENT",
        Unit::PERCENT,
        "Percentage of the whole track.",
    ),
];

/// Register the host type, every function and every constant namespace into
/// `engine`, and return the catalog snapshot describing them.
#[cfg(test)]
pub fn install(engine: &mut Engine) -> Catalog {
    install_with_host(engine, &Host::default())
}

pub fn install_with_host(engine: &mut Engine, host: &Host) -> Catalog {
    engine.build_type::<Unit>();
    for function in FUNCTIONS {
        function.install(engine, host);
    }
    let mut module = Module::new();
    for constant in CONSTANTS {
        module.set_var(constant.name, constant.value);
    }
    engine.register_static_module(CONSTANTS[0].namespace, module.into());
    Catalog {
        functions: FUNCTIONS.to_vec(),
        constants: CONSTANTS.to_vec(),
    }
}

/// Confirm that `engine` and `catalog` describe the same API.
///
/// Rhai exposes registered function metadata, so comparing it with the catalog
/// catches drift between a registration and its descriptor before a user meets
/// it in the prompt.
pub fn verify_parity(engine: &Engine, catalog: &Catalog) -> Result<(), String> {
    let signatures = engine.gen_fn_signatures(false);
    for function in catalog.functions() {
        if function.docs.is_empty() || function.returns.is_empty() {
            return Err(format!("{} has no documentation", function.name));
        }
        let signature = function.signature();
        if !signatures
            .iter()
            .any(|registered| registered.contains(&signature))
        {
            return Err(format!(
                "{signature} is registered but missing from the engine metadata"
            ));
        }
        for param in function.params {
            if !signature.contains(param.name) {
                return Err(format!("{signature} does not mention {}", param.name));
            }
        }
    }
    for constant in catalog.constants() {
        let qualified = constant.qualified_name();
        if constant.docs.is_empty() {
            return Err(format!("{qualified} has no documentation"));
        }
        if constant.value.is_percent() == (constant.name != "PERCENT") {
            return Err(format!("{qualified} mislabels the percentage value"));
        }
        let resolved: Unit = engine
            .eval(&format!("{}::{}", constant.namespace, constant.name))
            .map_err(|error| format!("{qualified} does not resolve: {error}"))?;
        if resolved != constant.value {
            return Err(format!("{qualified} resolves to a different value"));
        }
        if Unit::from_label(constant.name) != Some(constant.value) {
            return Err(format!("{qualified} has a mismatched canonical name"));
        }
    }
    for unit in Unit::all() {
        if !catalog
            .constants()
            .iter()
            .any(|constant| constant.value == unit)
        {
            return Err(format!("unit::{} is not exported", unit.label()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let mut engine = Engine::new();
        let catalog = install(&mut engine);
        assert_eq!(verify_parity(&engine, &catalog), Ok(()));
        catalog
    }

    #[test]
    fn registered_functions_are_callable_and_listed_in_the_catalog() {
        let mut engine = Engine::new();
        let catalog = install(&mut engine);
        let function = catalog
            .functions()
            .iter()
            .find(|function| function.name == "version")
            .expect("version is registered");
        assert_eq!(function.signature(), "version()");
        assert_eq!(function.returns, "string");
        assert!(function.params.is_empty());
        assert!(!function.docs.is_empty());
        let value: String = engine.eval("version()").unwrap();
        assert_eq!(value, format!("muse {}", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn unit_namespace_values_resolve_from_scripts_as_host_values() {
        let mut engine = Engine::new();
        install(&mut engine);
        for unit in Unit::all() {
            let value: rhai::Dynamic = engine
                .eval(&format!("unit::{}", unit.label()))
                .expect("registered constants resolve as namespace values");
            let resolved = value
                .clone()
                .try_cast::<Unit>()
                .expect("namespace values are host Unit values");
            assert_eq!(resolved, unit);
            // Unit values are not strings, and a string is not accepted.
            assert_ne!(value.type_name(), "string");
        }
        let wrong_unit: Result<Unit, _> = engine.eval("\"SEC\"");
        assert!(wrong_unit.is_err());
    }

    #[test]
    fn constant_tables_are_unique_and_documented() {
        let catalog = catalog();
        let names: Vec<String> = catalog
            .constants()
            .iter()
            .map(Constant::qualified_name)
            .collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(names.len(), unique.len());
        assert_eq!(
            names,
            vec![
                "unit::MS".to_owned(),
                "unit::SEC".to_owned(),
                "unit::MIN".to_owned(),
                "unit::HOUR".to_owned(),
                "unit::PERCENT".to_owned(),
            ]
        );
        assert!(
            catalog
                .constants()
                .iter()
                .all(|constant| !constant.docs.is_empty())
        );
    }

    #[test]
    fn unit_labels_round_trip_and_flag_percent() {
        for unit in Unit::all() {
            assert_eq!(Unit::from_label(unit.label()), Some(unit));
        }
        assert_eq!(Unit::from_label("SECONDS"), None);
        assert!(Unit::PERCENT.is_percent());
        assert!(!Unit::SEC.is_percent());
        assert_eq!(Unit::SEC.label(), "SEC");
    }

    #[test]
    fn a_drifted_catalog_is_reported_instead_of_silently_accepted() {
        let mut engine = Engine::new();
        let mut catalog = install(&mut engine);
        assert!(verify_parity(&engine, &catalog).is_ok());
        catalog
            .functions
            .push(Function::new("seek", &[], "()", "Seek.", |_, _| {}));
        let error = verify_parity(&engine, &catalog).unwrap_err();
        assert!(error.contains("seek()"), "unexpected error: {error}");
    }

    #[test]
    fn signatures_join_parameter_names() {
        let function = Function::new(
            "seek",
            &[
                Param {
                    name: "value",
                    role: ParamRole::Value,
                    docs: "Position.",
                },
                Param {
                    name: "unit",
                    role: ParamRole::Unit { percent: true },
                    docs: "Unit.",
                },
            ],
            "()",
            "Seek.",
            |_, _| {},
        );
        assert_eq!(function.signature(), "seek(value, unit)");
        assert_eq!(function.params[0].role, ParamRole::Value);
    }
}
