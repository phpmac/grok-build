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

#[cfg(test)]
mod tests {
    // 在 `cfg(test)` 下三门控必须为 false, 否则上游 layout 单测被本地 fork 误伤.
    // 生产二进制里它们为 true (见非 test 构建). 合并上游后若改掉 cfg 语义会挂这里.

    #[test]
    fn test_cfg_does_not_suppress_promo_chrome() {
        assert!(!super::suppress_announcements());
        assert!(!super::suppress_changelog());
        assert!(!super::suppress_logo());
    }
}
