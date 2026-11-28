pub fn enabled() -> bool {
    cfg!(debug_assertions) && std::env::var_os("STICKER_DROP_DIAGNOSTICS").is_some()
}
macro_rules! drop_diag {
    ($($arg:tt)*) => { if $crate::diagnostics::enabled() { eprintln!($($arg)*); } };
}
pub(crate) use drop_diag;
