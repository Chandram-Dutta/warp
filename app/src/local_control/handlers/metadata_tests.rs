use super::{SurfaceDestination, surface_unavailable_reason};
use crate::features::FeatureFlag;

#[test]
fn vertical_tabs_surface_reports_feature_flag_unavailable() {
    let flag_guard = FeatureFlag::VerticalTabs.override_enabled(false);
    warpui::App::test((), |mut app| async move {
        assert_eq!(
            app.update(|ctx| { surface_unavailable_reason(SurfaceDestination::VerticalTabs, ctx) }),
            Some("vertical tabs are unavailable or disabled")
        );
    });
    drop(flag_guard);
}
