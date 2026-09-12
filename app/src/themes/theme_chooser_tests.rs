use super::{ThemeKind, WarpThemeConfig, theme_chooser_items};

#[test]
fn bundled_reward_themes_are_available_without_an_account() {
    let items = theme_chooser_items(&WarpThemeConfig::new());
    for kind in [
        ThemeKind::SentReferralReward,
        ThemeKind::ReceivedReferralReward,
    ] {
        assert!(items.iter().any(|item| item.kind == kind));
    }
}
