//! Lint probe for `phasekit_xtask::tests::clippy_bans_fire` (PLAN.md M0.3). Never built in place: the test copies
//! this crate into a scratch workspace with the repository's `clippy.toml` and `[workspace.lints]` tables and runs
//! clippy with `-D warnings`, as gate G2 does. Every line marked `// fires: <rule>` breaks exactly that rule and must
//! get an error (naming the path, for a `clippy.toml` entry); no other line may get a diagnostic. The test also checks
//! that every `clippy.toml` path and every lint the workspace denies has a marked line here.
#![allow(missing_docs, reason = "probe items are not API")]

// clippy.toml `disallowed-methods`: f64 (D12, E16)

pub fn f64_mul_add(x: f64, y: f64, z: f64) -> f64 {
    x.mul_add(y, z) // fires: f64::mul_add
}

pub fn f64_powi(x: f64) -> f64 {
    x.powi(3) // fires: f64::powi
}

pub fn f64_powf(x: f64, y: f64) -> f64 {
    x.powf(y) // fires: f64::powf
}

pub fn f64_log(x: f64, y: f64) -> f64 {
    x.log(y) // fires: f64::log
}

pub fn f64_hypot(x: f64, y: f64) -> f64 {
    x.hypot(y) // fires: f64::hypot
}

pub fn f64_atan2(x: f64, y: f64) -> f64 {
    x.atan2(y) // fires: f64::atan2
}

pub fn f64_exp(x: f64) -> f64 {
    x.exp() // fires: f64::exp
}

pub fn f64_exp2(x: f64) -> f64 {
    x.exp2() // fires: f64::exp2
}

pub fn f64_exp_m1(x: f64) -> f64 {
    x.exp_m1() // fires: f64::exp_m1
}

pub fn f64_ln(x: f64) -> f64 {
    x.ln() // fires: f64::ln
}

pub fn f64_ln_1p(x: f64) -> f64 {
    x.ln_1p() // fires: f64::ln_1p
}

pub fn f64_log2(x: f64) -> f64 {
    x.log2() // fires: f64::log2
}

pub fn f64_log10(x: f64) -> f64 {
    x.log10() // fires: f64::log10
}

pub fn f64_sqrt(x: f64) -> f64 {
    x.sqrt() // fires: f64::sqrt
}

pub fn f64_cbrt(x: f64) -> f64 {
    x.cbrt() // fires: f64::cbrt
}

pub fn f64_sin(x: f64) -> f64 {
    x.sin() // fires: f64::sin
}

pub fn f64_cos(x: f64) -> f64 {
    x.cos() // fires: f64::cos
}

pub fn f64_tan(x: f64) -> f64 {
    x.tan() // fires: f64::tan
}

pub fn f64_asin(x: f64) -> f64 {
    x.asin() // fires: f64::asin
}

pub fn f64_acos(x: f64) -> f64 {
    x.acos() // fires: f64::acos
}

pub fn f64_atan(x: f64) -> f64 {
    x.atan() // fires: f64::atan
}

pub fn f64_sinh(x: f64) -> f64 {
    x.sinh() // fires: f64::sinh
}

pub fn f64_cosh(x: f64) -> f64 {
    x.cosh() // fires: f64::cosh
}

pub fn f64_tanh(x: f64) -> f64 {
    x.tanh() // fires: f64::tanh
}

pub fn f64_asinh(x: f64) -> f64 {
    x.asinh() // fires: f64::asinh
}

pub fn f64_acosh(x: f64) -> f64 {
    x.acosh() // fires: f64::acosh
}

pub fn f64_atanh(x: f64) -> f64 {
    x.atanh() // fires: f64::atanh
}

// clippy.toml `disallowed-methods`: the environment (ROT-015, ROT-016, ROT-153)

pub fn env_var() -> Result<String, std::env::VarError> {
    std::env::var("PHASEKIT") // fires: std::env::var
}

pub fn env_var_os() -> Option<std::ffi::OsString> {
    std::env::var_os("PHASEKIT") // fires: std::env::var_os
}

pub fn env_vars() -> usize {
    std::env::vars().count() // fires: std::env::vars
}

pub fn env_vars_os() -> usize {
    std::env::vars_os().count() // fires: std::env::vars_os
}

pub fn env_home_dir() -> Option<std::path::PathBuf> {
    std::env::home_dir() // fires: std::env::home_dir
}

// clippy.toml `disallowed-methods`: the filesystem (ROT-153)

pub fn fs_canonicalize() -> std::io::Result<std::path::PathBuf> {
    std::fs::canonicalize("a") // fires: std::fs::canonicalize
}

pub fn fs_copy() -> std::io::Result<u64> {
    std::fs::copy("a", "b") // fires: std::fs::copy
}

pub fn fs_create_dir() -> std::io::Result<()> {
    std::fs::create_dir("a") // fires: std::fs::create_dir
}

pub fn fs_create_dir_all() -> std::io::Result<()> {
    std::fs::create_dir_all("a") // fires: std::fs::create_dir_all
}

pub fn fs_exists() -> std::io::Result<bool> {
    std::fs::exists("a") // fires: std::fs::exists
}

pub fn fs_hard_link() -> std::io::Result<()> {
    std::fs::hard_link("a", "b") // fires: std::fs::hard_link
}

pub fn fs_metadata() -> std::io::Result<std::fs::Metadata> {
    std::fs::metadata("a") // fires: std::fs::metadata
}

pub fn fs_read() -> std::io::Result<Vec<u8>> {
    std::fs::read("a") // fires: std::fs::read
}

pub fn fs_read_dir() -> std::io::Result<std::fs::ReadDir> {
    std::fs::read_dir("a") // fires: std::fs::read_dir
}

pub fn fs_read_link() -> std::io::Result<std::path::PathBuf> {
    std::fs::read_link("a") // fires: std::fs::read_link
}

pub fn fs_read_to_string() -> std::io::Result<String> {
    std::fs::read_to_string("a") // fires: std::fs::read_to_string
}

pub fn fs_remove_dir() -> std::io::Result<()> {
    std::fs::remove_dir("a") // fires: std::fs::remove_dir
}

pub fn fs_remove_dir_all() -> std::io::Result<()> {
    std::fs::remove_dir_all("a") // fires: std::fs::remove_dir_all
}

pub fn fs_remove_file() -> std::io::Result<()> {
    std::fs::remove_file("a") // fires: std::fs::remove_file
}

pub fn fs_rename() -> std::io::Result<()> {
    std::fs::rename("a", "b") // fires: std::fs::rename
}

pub fn fs_symlink_metadata() -> std::io::Result<std::fs::Metadata> {
    std::fs::symlink_metadata("a") // fires: std::fs::symlink_metadata
}

pub fn fs_write() -> std::io::Result<()> {
    std::fs::write("a", b"x") // fires: std::fs::write
}

pub fn fs_set_permissions(perm: std::fs::Permissions) -> std::io::Result<()> {
    std::fs::set_permissions("a", perm) // fires: std::fs::set_permissions
}

pub fn fs_file_open() -> std::io::Result<std::fs::File> {
    std::fs::File::open("a") // fires: std::fs::File::open
}

pub fn fs_file_create() -> std::io::Result<std::fs::File> {
    std::fs::File::create("a") // fires: std::fs::File::create
}

pub fn fs_file_create_new() -> std::io::Result<std::fs::File> {
    std::fs::File::create_new("a") // fires: std::fs::File::create_new
}

pub fn fs_open_options_open(options: &std::fs::OpenOptions) -> std::io::Result<std::fs::File> {
    options.open("a") // fires: std::fs::OpenOptions::open
}

pub fn fs_dir_builder_create(builder: &std::fs::DirBuilder) -> std::io::Result<()> {
    builder.create("a") // fires: std::fs::DirBuilder::create
}

pub fn path_canonicalize(path: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    path.canonicalize() // fires: std::path::Path::canonicalize
}

pub fn path_exists(path: &std::path::Path) -> bool {
    path.exists() // fires: std::path::Path::exists
}

pub fn path_is_dir(path: &std::path::Path) -> bool {
    path.is_dir() // fires: std::path::Path::is_dir
}

pub fn path_is_file(path: &std::path::Path) -> bool {
    path.is_file() // fires: std::path::Path::is_file
}

pub fn path_is_symlink(path: &std::path::Path) -> bool {
    path.is_symlink() // fires: std::path::Path::is_symlink
}

pub fn path_metadata(path: &std::path::Path) -> std::io::Result<std::fs::Metadata> {
    path.metadata() // fires: std::path::Path::metadata
}

pub fn path_read_dir(path: &std::path::Path) -> std::io::Result<std::fs::ReadDir> {
    path.read_dir() // fires: std::path::Path::read_dir
}

pub fn path_read_link(path: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    path.read_link() // fires: std::path::Path::read_link
}

pub fn path_symlink_metadata(path: &std::path::Path) -> std::io::Result<std::fs::Metadata> {
    path.symlink_metadata() // fires: std::path::Path::symlink_metadata
}

pub fn path_try_exists(path: &std::path::Path) -> std::io::Result<bool> {
    path.try_exists() // fires: std::path::Path::try_exists
}

// clippy.toml `disallowed-types`: locks and cells (D8, ROT-015, ROT-032)

pub fn type_mutex(_: std::sync::Mutex<u8>) {} // fires: std::sync::Mutex

pub fn type_rwlock(_: std::sync::RwLock<u8>) {} // fires: std::sync::RwLock

pub fn type_cell(_: std::cell::Cell<u8>) {} // fires: std::cell::Cell

pub fn type_refcell(_: std::cell::RefCell<u8>) {} // fires: std::cell::RefCell

pub fn type_oncecell(_: std::cell::OnceCell<u8>) {} // fires: std::cell::OnceCell

// clippy.toml `disallowed-types`: atomics (ROT-031)

pub fn type_atomic_bool(_: std::sync::atomic::AtomicBool) {} // fires: std::sync::atomic::AtomicBool

pub fn type_atomic_i8(_: std::sync::atomic::AtomicI8) {} // fires: std::sync::atomic::AtomicI8

pub fn type_atomic_i16(_: std::sync::atomic::AtomicI16) {} // fires: std::sync::atomic::AtomicI16

pub fn type_atomic_i32(_: std::sync::atomic::AtomicI32) {} // fires: std::sync::atomic::AtomicI32

pub fn type_atomic_i64(_: std::sync::atomic::AtomicI64) {} // fires: std::sync::atomic::AtomicI64

pub fn type_atomic_isize(_: std::sync::atomic::AtomicIsize) {} // fires: std::sync::atomic::AtomicIsize

pub fn type_atomic_u8(_: std::sync::atomic::AtomicU8) {} // fires: std::sync::atomic::AtomicU8

pub fn type_atomic_u16(_: std::sync::atomic::AtomicU16) {} // fires: std::sync::atomic::AtomicU16

pub fn type_atomic_u32(_: std::sync::atomic::AtomicU32) {} // fires: std::sync::atomic::AtomicU32

pub fn type_atomic_u64(_: std::sync::atomic::AtomicU64) {} // fires: std::sync::atomic::AtomicU64

pub fn type_atomic_usize(_: std::sync::atomic::AtomicUsize) {} // fires: std::sync::atomic::AtomicUsize

pub fn type_atomic_ptr(_: std::sync::atomic::AtomicPtr<u8>) {} // fires: std::sync::atomic::AtomicPtr

// clippy.toml `disallowed-macros` (D8, ROT-268)

pub fn macro_thread_local() -> u8 {
    std::thread_local!(static SCRATCH: u8 = const { 0 }); // fires: std::thread_local
    SCRATCH.with(|s| *s)
}

// Workspace lints denied in Cargo.toml: printing (ROT-024)

pub fn lint_print() {
    print!("x"); // fires: clippy::print_stdout
}

pub fn lint_println() {
    println!("x"); // fires: clippy::print_stdout
}

pub fn lint_eprint() {
    eprint!("x"); // fires: clippy::print_stderr
}

pub fn lint_eprintln() {
    eprintln!("x"); // fires: clippy::print_stderr
}

// Workspace lints denied in Cargo.toml: panics and debugging (D12, S-06)

pub fn lint_unwrap(x: Option<u8>) -> u8 {
    x.unwrap() // fires: clippy::unwrap_used
}

pub fn lint_expect(x: Option<u8>) -> u8 {
    x.expect("x") // fires: clippy::expect_used
}

pub fn lint_todo() -> u8 {
    todo!() // fires: clippy::todo
}

pub fn lint_unimplemented() -> u8 {
    unimplemented!() // fires: clippy::unimplemented
}

pub fn lint_panic() -> u8 {
    panic!("x") // fires: clippy::panic
}

pub fn lint_dbg(x: u8) -> u8 {
    dbg!(x) // fires: clippy::dbg_macro
}

// Tautological assertions (ROT-294): `clippy::all` lints, errors under `-D warnings`. `assert_eq!(f(), f())` gets
// past `eq_op` (a call may have side effects); `gates assertions` rejects it.
pub fn taut_constant() {
    assert!(true); // fires: clippy::assertions_on_constants
}

pub fn taut_same_operands(x: u8) {
    assert_eq!(x, x); // fires: clippy::eq_op
}

pub fn taut_bool_comparison(x: bool) {
    assert_eq!(x, true); // fires: clippy::bool_assert_comparison
}

// rustc `dead_code` (ROT-018): an option field nobody reads is a warning, an error under `-D warnings`

pub struct Options {
    read: u8,
    unread: u8, // fires: dead_code
}

impl Options {
    pub fn read(&self) -> u8 {
        self.read
    }
}
