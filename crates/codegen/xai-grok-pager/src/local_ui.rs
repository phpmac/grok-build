//! Local Grok Build product UI gates.
//!
//! Production (non-test) builds hide welcome promo chrome that depends on
//! upstream CDN / remote settings. Unit tests keep the upstream behavior so
//! layout and merge logic stay covered.

/// Suppress welcome/session announcements (remote promo, config layers).
pub fn suppress_announcements() -> bool {
    !cfg!(test)
}

/// Suppress welcome changelog block, menu row, and changelog preloads.
pub fn suppress_changelog() -> bool {
    !cfg!(test)
}

/// Suppress welcome braille logo (full / small / minimal compact).
pub fn suppress_logo() -> bool {
    !cfg!(test)
}
