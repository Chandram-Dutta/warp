use settings_page::{FilteredPageType, MatchData, PageType, SettingsWidget, search_terms_match};
use warpui::elements::Empty;
use warpui::{App, AppContext, Element, Entity, View};

use super::*;
use crate::appearance::Appearance;
#[cfg(feature = "local_only")]
use crate::settings::DefaultSessionMode;

// ── MatchData behavior ──────────────────────────────────────────────────────

#[test]
fn match_data_uncounted_true_is_truthy() {
    assert!(MatchData::Uncounted(true).is_truthy());
}

#[test]
fn match_data_uncounted_false_is_not_truthy() {
    assert!(!MatchData::Uncounted(false).is_truthy());
}

#[test]
fn match_data_countable_nonzero_is_truthy() {
    assert!(MatchData::Countable(3).is_truthy());
    assert!(MatchData::Countable(1).is_truthy());
}

#[test]
fn match_data_countable_zero_is_not_truthy() {
    assert!(!MatchData::Countable(0).is_truthy());
}

// ── Display labels ─────────────────────────────────────────────────

#[test]
fn subpage_display_names_are_correct() {
    assert_eq!(SettingsSection::WarpAgent.to_string(), "Warp Agent");
    assert_eq!(
        SettingsSection::CloudEnvironments.to_string(),
        "Environments"
    );
}

// ── slug / from_slug ───────────────────────────────────────────────

/// Every `SettingsSection` variant.
///
/// `all_sections_list_is_exhaustive` keeps this honest: adding a variant
/// breaks the exhaustive match there, which is the prompt to add it here.
const ALL_SECTIONS: &[SettingsSection] = &[
    SettingsSection::About,
    #[cfg(feature = "local_only")]
    SettingsSection::LocalAI,
    SettingsSection::BillingAndUsage,
    SettingsSection::Appearance,
    SettingsSection::Features,
    SettingsSection::Keybindings,
    SettingsSection::Privacy,
    SettingsSection::Scripting,
    SettingsSection::Warpify,
    SettingsSection::WarpAgent,
    SettingsSection::CloudEnvironments,
];

#[test]
fn all_sections_list_is_exhaustive() {
    fn is_listed(section: SettingsSection) -> bool {
        let known = match section {
            #[cfg(feature = "local_only")]
            SettingsSection::LocalAI => section,
            SettingsSection::About
            | SettingsSection::BillingAndUsage
            | SettingsSection::Appearance
            | SettingsSection::Features
            | SettingsSection::Keybindings
            | SettingsSection::Privacy
            | SettingsSection::Scripting
            | SettingsSection::Warpify
            | SettingsSection::WarpAgent
            | SettingsSection::CloudEnvironments => section,
        };
        ALL_SECTIONS.contains(&known)
    }

    for section in ALL_SECTIONS {
        assert!(is_listed(*section), "{section:?} is missing from the list");
    }
}

#[test]
#[cfg(feature = "local_only")]
fn local_only_settings_allow_list_excludes_cloud_and_agent_pages() {
    assert!(SettingsSection::LocalAI.is_available());
    assert!(SettingsSection::Appearance.is_available());
    assert!(!SettingsSection::WarpAgent.is_available());
}

#[test]
#[cfg(feature = "local_only")]
fn local_only_settings_actions_and_events_reject_direct_bypasses() {
    assert!(SettingsAction::SelectAndRefresh(SettingsSection::LocalAI).is_available_in_product());
    assert!(
        !SettingsAction::SelectAndRefresh(SettingsSection::WarpAgent).is_available_in_product()
    );

    assert!(SettingsViewEvent::StartResize.is_available_in_product());
    assert!(!SettingsViewEvent::CheckForUpdate.is_available_in_product());

    assert!(
        FeaturesPageAction::SetDefaultSessionMode(DefaultSessionMode::Terminal)
            .is_available_in_product()
    );
    assert!(
        !FeaturesPageAction::SetDefaultSessionMode(DefaultSessionMode::Agent)
            .is_available_in_product()
    );
    assert!(!FeaturesPageAction::ToggleCodeAsDefaultEditor.is_available_in_product());
    assert!(!FeaturesPageAction::ToggleAgentTaskCompletedNotifications.is_available_in_product());

    assert!(
        SettingsAction::AppearancePageToggle(AppearancePageAction::ToggleVerticalTabs)
            .is_available_in_product()
    );
    assert!(
        SettingsAction::AppearancePageToggle(AppearancePageAction::SetFontSize)
            .is_available_in_product()
    );
}

#[test]
fn retained_sections_round_trip_through_their_slugs() {
    for section in [
        SettingsSection::About,
        #[cfg(feature = "local_only")]
        SettingsSection::LocalAI,
        SettingsSection::Appearance,
        SettingsSection::Features,
        SettingsSection::Keybindings,
        SettingsSection::Warpify,
    ] {
        assert_eq!(
            SettingsSection::from_slug(section.slug()),
            Some(section),
            "{section:?} should round-trip through its slug"
        );
    }
}

#[test]
fn slugs_are_unique_across_sections() {
    let mut slugs: Vec<&str> = ALL_SECTIONS.iter().map(|section| section.slug()).collect();
    let total = slugs.len();
    slugs.sort_unstable();
    slugs.dedup();
    assert_eq!(slugs.len(), total, "two sections share a slug");
}

#[test]
fn slugs_were_seeded_from_the_display_labels_they_replaced() {
    // Slugs were seeded from the Display strings that used to double as the
    // persistence key, so no data migration was needed. Display is now free to
    // diverge; if it does, update this test rather than the slugs, which are a
    // stored contract.
    for section in ALL_SECTIONS {
        assert_eq!(
            section.slug(),
            section.to_string(),
            "{section:?} slug diverged from the Display label it was seeded from"
        );
    }
}

#[test]
fn removed_product_slugs_and_legacy_aliases_are_unavailable() {
    for slug in [
        "Account",
        "Billing and usage",
        "Warp Agent",
        "Oz",
        "AI",
        "Warp Drive",
        "WarpDrive",
        "Profiles",
        "AgentProfiles",
        "MCP servers",
        "MCP Servers",
        "AgentMCPServers",
        "Knowledge",
        "Third party CLI agents",
        "ThirdPartyCLIAgents",
        "Editor and Code Review",
        "EditorAndCodeReview",
        "Environments",
        "CloudEnvironments",
        "Oz Cloud API Keys",
        "OzCloudAPIKeys",
        "Referrals",
        "Teams",
        "Shared blocks",
    ] {
        assert_eq!(SettingsSection::from_slug(slug), None, "{slug}");
    }
}

#[test]
fn from_slug_rejects_unknown_input() {
    assert_eq!(SettingsSection::from_slug("Not a page"), None);
    assert_eq!(SettingsSection::from_slug(""), None);
    for removed in ["Code", "CodeIndexing", "Indexing and projects"] {
        assert_eq!(SettingsSection::from_slug(removed), None);
    }
}

// ── Collapsed umbrella nav-stop behavior ────────────────────────────────────
// Verify that arrow-key navigation lands on a collapsed umbrella as a single
// stop (and activates it by jumping to the first subpage, which auto-expands
// the umbrella) instead of silently skipping over it.

use nav::{SettingsNavItem, SettingsUmbrella};

/// Fixed synthetic subpages for testing navigation independently of product membership.
const AGENT_SUBPAGES: &[SettingsSection] = &[
    SettingsSection::WarpAgent,
    SettingsSection::Features,
    SettingsSection::Scripting,
    SettingsSection::Keybindings,
    SettingsSection::Warpify,
];

/// Exercises both single-page and multi-page groups independently of product navigation.
fn realistic_nav_items() -> Vec<SettingsNavItem> {
    vec![
        SettingsNavItem::Page(SettingsSection::About),
        SettingsNavItem::Umbrella(SettingsUmbrella::new("Agents", AGENT_SUBPAGES.to_vec())),
        SettingsNavItem::Page(SettingsSection::BillingAndUsage),
        SettingsNavItem::Umbrella(SettingsUmbrella::new(
            "Appearance",
            vec![SettingsSection::Appearance],
        )),
        SettingsNavItem::Umbrella(SettingsUmbrella::new(
            "Cloud platform",
            vec![SettingsSection::CloudEnvironments, SettingsSection::Privacy],
        )),
        SettingsNavItem::Page(SettingsSection::Warpify),
    ]
}

/// Mutably flips an umbrella's `expanded` flag at `nav_index`.
fn set_expanded(nav_items: &mut [SettingsNavItem], nav_index: usize, expanded: bool) {
    if let Some(SettingsNavItem::Umbrella(u)) = nav_items.get_mut(nav_index) {
        u.expanded = expanded;
    } else {
        panic!("nav_items[{nav_index}] is not an Umbrella");
    }
}

#[test]
fn collapsed_umbrella_is_a_single_nav_stop() {
    let nav_items = realistic_nav_items();
    // All umbrellas default to collapsed.
    let stops = build_nav_stops(&nav_items, |_| true);

    // Expect: About, <Agents umbrella>, BillingAndUsage, <Appearance umbrella>,
    // <Cloud platform umbrella>, Warpify.
    assert_eq!(stops.len(), 6);
    assert!(matches!(stops[0], NavStop::Section(SettingsSection::About)));
    assert!(matches!(
        stops[1],
        NavStop::CollapsedUmbrella {
            nav_index: 1,
            first_subpage: SettingsSection::WarpAgent,
            last_subpage: SettingsSection::Warpify,
        }
    ));
    assert!(matches!(
        stops[2],
        NavStop::Section(SettingsSection::BillingAndUsage)
    ));
    assert!(matches!(
        stops[3],
        NavStop::CollapsedUmbrella {
            nav_index: 3,
            first_subpage: SettingsSection::Appearance,
            last_subpage: SettingsSection::Appearance,
        }
    ));
    assert!(matches!(
        stops[4],
        NavStop::CollapsedUmbrella {
            nav_index: 4,
            first_subpage: SettingsSection::CloudEnvironments,
            last_subpage: SettingsSection::Privacy,
        }
    ));
    assert!(matches!(
        stops[5],
        NavStop::Section(SettingsSection::Warpify)
    ));
}

#[test]
fn expanded_umbrella_produces_section_stop_per_subpage() {
    let mut nav_items = realistic_nav_items();
    // Expand the Agents umbrella so each of its subpages becomes a nav stop.
    set_expanded(&mut nav_items, 1, true);

    let stops = build_nav_stops(&nav_items, |_| true);

    // Expect: About, WarpAgent, Features, Scripting, Keybindings,
    // Warpify, BillingAndUsage, <Appearance umbrella>,
    // <Cloud platform umbrella>, Teams.
    let sections: Vec<_> = stops
        .iter()
        .map(|s| match s {
            NavStop::Section(section) => format!("{section:?}"),
            NavStop::CollapsedUmbrella { nav_index, .. } => format!("Umbrella@{nav_index}"),
        })
        .collect();
    assert_eq!(
        sections,
        vec![
            "About",
            "WarpAgent",
            "Features",
            "Scripting",
            "Keybindings",
            "Warpify",
            "BillingAndUsage",
            "Umbrella@3",
            "Umbrella@4",
            "Warpify",
        ]
    );
}

#[test]
fn collapsed_umbrella_with_filtered_subpages_uses_first_visible_subpage() {
    // When a search filter hides the first subpage, activating the collapsed
    // umbrella should land on the *next* visible subpage (still auto-expanding).
    let nav_items = realistic_nav_items();

    let stops = build_nav_stops(&nav_items, |section| {
        // Hide WarpAgent (first AI subpage); keep the rest.
        section != SettingsSection::WarpAgent
    });

    let agents_stop = stops
        .iter()
        .find(|s| matches!(s, NavStop::CollapsedUmbrella { nav_index: 1, .. }))
        .expect("Agents umbrella should still be a collapsed stop");

    match agents_stop {
        NavStop::CollapsedUmbrella {
            first_subpage,
            last_subpage,
            ..
        } => {
            assert_eq!(
                *first_subpage,
                SettingsSection::Features,
                "WarpAgent is hidden by the filter, so the first visible subpage is Features"
            );
            assert_eq!(
                *last_subpage,
                SettingsSection::Warpify,
                "last_subpage is unaffected by hiding WarpAgent and should remain the last visible subpage"
            );
        }
        _ => unreachable!(),
    }
}

#[test]
fn umbrella_with_no_visible_subpages_is_skipped_entirely() {
    let nav_items = realistic_nav_items();

    let stops = build_nav_stops(&nav_items, |section| !AGENT_SUBPAGES.contains(&section));

    // The Agents umbrella's subpages are all hidden, so the entire umbrella
    // should be absent from the nav order.
    assert!(
        stops
            .iter()
            .all(|s| !matches!(s, NavStop::CollapsedUmbrella { nav_index: 1, .. })),
        "Agents umbrella should not appear when none of its subpages are visible"
    );
    // The still-visible Code / Cloud platform umbrellas remain as stops.
    assert!(
        stops
            .iter()
            .any(|s| matches!(s, NavStop::CollapsedUmbrella { nav_index: 3, .. }))
    );
    assert!(
        stops
            .iter()
            .any(|s| matches!(s, NavStop::CollapsedUmbrella { nav_index: 4, .. }))
    );
}

#[test]
fn filtered_out_top_level_page_is_skipped() {
    let nav_items = realistic_nav_items();

    let stops = build_nav_stops(&nav_items, |section| section != SettingsSection::Warpify);

    assert!(
        !stops
            .iter()
            .any(|s| matches!(s, NavStop::Section(SettingsSection::Warpify))),
        "Warpify should be filtered out entirely"
    );
    // But other pages remain.
    assert!(
        stops
            .iter()
            .any(|s| matches!(s, NavStop::Section(SettingsSection::About)))
    );
}

// ── current_stop_index ──────────────────────────────────────────────────────

#[test]
fn current_stop_index_matches_section_stop() {
    let nav_items = realistic_nav_items();
    let stops = build_nav_stops(&nav_items, |_| true);

    let idx = current_stop_index(&stops, &nav_items, SettingsSection::BillingAndUsage);
    assert_eq!(idx, Some(2));
}

#[test]
fn current_stop_index_maps_subpage_to_collapsed_umbrella() {
    // Edge case: the user manually collapsed the Agents umbrella while still
    // on one of its subpages. The collapsed umbrella should match as the
    // current stop so arrow-key cycling continues from the umbrella's position.
    let nav_items = realistic_nav_items();
    let stops = build_nav_stops(&nav_items, |_| true);

    let idx = current_stop_index(&stops, &nav_items, SettingsSection::Keybindings);
    assert_eq!(
        idx,
        Some(1),
        "Keybindings is under the collapsed synthetic umbrella at nav_index 1"
    );
}

#[test]
fn current_stop_index_returns_none_when_section_is_not_present() {
    let nav_items = realistic_nav_items();
    // Filter out all Agents subpages (and therefore the umbrella) entirely.
    let stops = build_nav_stops(&nav_items, |section| !AGENT_SUBPAGES.contains(&section));

    // Keybindings isn't directly in stops, and no remaining collapsed umbrella
    // contains it, so current_stop_index should return None.
    assert_eq!(
        current_stop_index(&stops, &nav_items, SettingsSection::Keybindings),
        None
    );
}

// ── next_stop_index wrapping ────────────────────────────────────────────────

#[test]
fn next_stop_index_wraps_at_ends() {
    assert_eq!(next_stop_index(0, 3, CycleDirection::Up), 2);
    assert_eq!(next_stop_index(2, 3, CycleDirection::Down), 0);
    assert_eq!(next_stop_index(1, 3, CycleDirection::Up), 0);
    assert_eq!(next_stop_index(1, 3, CycleDirection::Down), 2);
}

#[test]
fn next_stop_index_handles_single_stop() {
    assert_eq!(next_stop_index(0, 1, CycleDirection::Up), 0);
    assert_eq!(next_stop_index(0, 1, CycleDirection::Down), 0);
}

// ── End-to-end cycling (no search) ──────────────────────────────────────────
// These tests simulate the sequence of nav-stop activations that would result
// from repeatedly pressing Down/Up, ensuring a collapsed umbrella is never
// skipped over.

/// Computes the section that would become active after applying the direction
/// once, starting from `current`. Mirrors the final target-resolution step in
/// `cycle_pages`.
fn simulate_cycle(
    nav_items: &[SettingsNavItem],
    stops: &[NavStop],
    current: SettingsSection,
    direction: CycleDirection,
) -> SettingsSection {
    let active = current_stop_index(stops, nav_items, current)
        .expect("current should exist in stops in these tests");
    let next = next_stop_index(active, stops.len(), direction);
    match stops[next] {
        NavStop::Section(section) => section,
        NavStop::CollapsedUmbrella {
            first_subpage,
            last_subpage,
            ..
        } => match direction {
            CycleDirection::Up => last_subpage,
            CycleDirection::Down => first_subpage,
        },
    }
}

#[test]
fn arrow_down_from_about_with_collapsed_agents_lands_on_first_subpage() {
    let nav_items = realistic_nav_items();
    let stops = build_nav_stops(&nav_items, |_| true);

    // Pressing Down from About should auto-expand Agents and select WarpAgent,
    // not skip over to BillingAndUsage.
    let next = simulate_cycle(
        &nav_items,
        &stops,
        SettingsSection::About,
        CycleDirection::Down,
    );
    assert_eq!(next, SettingsSection::WarpAgent);
}

#[test]
fn arrow_up_from_billing_and_usage_with_collapsed_agents_lands_on_last_subpage() {
    let nav_items = realistic_nav_items();
    let stops = build_nav_stops(&nav_items, |_| true);

    // Pressing Up from BillingAndUsage should land on the collapsed Agents
    // umbrella, which resolves to Warpify (last visible subpage)
    // so the user continues moving in natural reading order rather than being
    // jumped back to the top of the umbrella.
    let next = simulate_cycle(
        &nav_items,
        &stops,
        SettingsSection::BillingAndUsage,
        CycleDirection::Up,
    );
    assert_eq!(next, SettingsSection::Warpify);
}

#[test]
fn arrow_up_into_collapsed_umbrella_respects_search_filter_for_last_subpage() {
    let nav_items = realistic_nav_items();
    // Hide the last two AI subpages; the last *visible* subpage of the
    // still-collapsed Agents umbrella should be Scripting.
    let is_visible = |section: SettingsSection| {
        !matches!(
            section,
            SettingsSection::Keybindings | SettingsSection::Warpify
        )
    };
    let stops = build_nav_stops(&nav_items, is_visible);

    // From BillingAndUsage, Up should land on the last *visible* AI subpage
    // (Scripting), not on the filtered-out Keybindings/Warpify
    // or on the first subpage WarpAgent.
    let next = simulate_cycle(
        &nav_items,
        &stops,
        SettingsSection::BillingAndUsage,
        CycleDirection::Up,
    );
    assert_eq!(next, SettingsSection::Scripting);
}

#[test]
fn arrow_down_from_expanded_last_subpage_leaves_umbrella() {
    let mut nav_items = realistic_nav_items();
    set_expanded(&mut nav_items, 1, true); // expand Agents
    let stops = build_nav_stops(&nav_items, |_| true);

    // Warpify is the last synthetic subpage; Down should move to
    // BillingAndUsage (the next top-level page in the nav order).
    let next = simulate_cycle(
        &nav_items,
        &stops,
        SettingsSection::Warpify,
        CycleDirection::Down,
    );
    assert_eq!(next, SettingsSection::BillingAndUsage);
}

#[test]
fn arrow_down_across_adjacent_collapsed_umbrellas() {
    let nav_items = realistic_nav_items();
    // Both Appearance and Cloud platform umbrellas are collapsed.
    let stops = build_nav_stops(&nav_items, |_| true);

    // From BillingAndUsage, Down should land on the Appearance subpage.
    let next_after_billing = simulate_cycle(
        &nav_items,
        &stops,
        SettingsSection::BillingAndUsage,
        CycleDirection::Down,
    );
    assert_eq!(next_after_billing, SettingsSection::Appearance);

    // From the Appearance umbrella stop (i.e. the user is "on" Appearance which
    // maps back to the collapsed umbrella), pressing Down again should land
    // on the Cloud platform umbrella's first subpage.
    let next_after_appearance = simulate_cycle(
        &nav_items,
        &stops,
        SettingsSection::Appearance,
        CycleDirection::Down,
    );
    assert_eq!(next_after_appearance, SettingsSection::CloudEnvironments);
}

#[test]
fn arrow_down_collapsed_umbrella_respects_search_filter() {
    let nav_items = realistic_nav_items();
    // Search filter hides WarpAgent and Features so the first visible
    // subpage is Scripting.
    let is_visible = |section: SettingsSection| {
        !matches!(
            section,
            SettingsSection::WarpAgent | SettingsSection::Features
        )
    };
    let stops = build_nav_stops(&nav_items, is_visible);

    // From About, Down should land on Scripting (first visible
    // subpage of the still-collapsed Agents umbrella), not on WarpAgent /
    // Features.
    let next = simulate_cycle(
        &nav_items,
        &stops,
        SettingsSection::About,
        CycleDirection::Down,
    );
    assert_eq!(next, SettingsSection::Scripting);
}

// ── PageType filter lifecycle across a rebuild (APP-4922) ────────────────────
// Rebuilding a page's PageType resets its widget filter to every widget, so an
// active query has to be reapplied for only matching widgets to render. No page
// rebuilds itself on navigation any more (each subpage owns its own view), but
// these tests still pin the underlying PageType::Uncategorized filter lifecycle
// and the real search_terms_match predicate that the invariant rests on.

/// Minimal View so PageType<V> can be instantiated in a unit test without the
/// full SettingsView/ViewContext a real settings page requires.
struct TestSettingsView;

impl Entity for TestSettingsView {
    type Event = ();
}

impl View for TestSettingsView {
    fn ui_name() -> &'static str {
        "TestSettingsView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn Element> {
        Empty::new().finish()
    }
}

/// A SettingsWidget whose only test-relevant state is its search terms; render
/// is never invoked by the filter lifecycle under test.
struct StubWidget {
    terms: &'static str,
}

impl SettingsWidget for StubWidget {
    type View = TestSettingsView;

    fn search_terms(&self) -> &str {
        self.terms
    }

    fn render(&self, _: &Self::View, _: &Appearance, _: &AppContext) -> Box<dyn Element> {
        Empty::new().finish()
    }
}

/// A fresh Uncategorized page mirroring build_page -> new_uncategorized: every
/// widget index visible by default.
fn stub_widgets_page() -> PageType<TestSettingsView> {
    let widgets: Vec<Box<dyn SettingsWidget<View = TestSettingsView>>> = vec![
        Box::new(StubWidget {
            terms: "warp agent global ai toggle",
        }),
        Box::new(StubWidget {
            terms: "active ai autosuggestions prompt",
        }),
        Box::new(StubWidget {
            terms: "ai input model api key",
        }),
        Box::new(StubWidget {
            terms: "file search fuzzy opener",
        }),
        Box::new(StubWidget {
            terms: "voice input",
        }),
    ];
    PageType::new_uncategorized(widgets, None)
}

/// Number of widgets the page would render under its current filter.
fn visible_widget_count<V: View>(page: &PageType<V>) -> usize {
    let FilteredPageType::Uncategorized { widgets, .. } = page.get_filtered() else {
        panic!("expected Uncategorized page");
    };
    widgets.len()
}

#[test]
fn search_terms_match_direct_unit_checks() {
    // Empty query matches everything (mirrors PageType::update_filter's guard).
    assert!(search_terms_match("warp agent global ai toggle", ""));
    // All-words, case-insensitive, non-contiguous.
    assert!(search_terms_match(
        "active ai autosuggestions prompt",
        "autosuggestions"
    ));
    assert!(search_terms_match(
        "active ai autosuggestions prompt",
        "ACTIVE AI"
    ));
    assert!(search_terms_match(
        "file search fuzzy opener",
        "file search"
    ));
    // Every word must appear.
    assert!(!search_terms_match(
        "warp agent global ai toggle",
        "file search"
    ));
    assert!(!search_terms_match(
        "active ai autosuggestions prompt",
        "autosuggestions key"
    ));
}

#[test]
fn rebuild_resets_filter_to_all_widgets() {
    // Searching "file search" matches exactly one widget. A freshly built page
    // (mirroring build_page -> new_uncategorized) resets the filter to every
    // widget, so without reapplying update_filter the page would show all
    // widgets.
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let mut page = stub_widgets_page();
            let md = page.update_filter("file search", ctx);
            assert!(md.is_truthy());
            assert_eq!(visible_widget_count(&page), 1);

            let rebuilt = stub_widgets_page();
            assert_eq!(
                visible_widget_count(&rebuilt),
                5,
                "rebuild resets the filter to all widgets when update_filter isn't reapplied"
            );
        });
    });
}

#[test]
fn rebuild_with_reapply_keeps_only_matching_widgets() {
    // The fix: after a rebuild, reapply update_filter with the active query so
    // only matching widgets render on the restored subpage.
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let mut page = stub_widgets_page();
            page.update_filter("file search", ctx);
            assert_eq!(visible_widget_count(&page), 1);

            let mut rebuilt = stub_widgets_page();
            rebuilt.update_filter("file search", ctx);
            assert_eq!(
                visible_widget_count(&rebuilt),
                1,
                "reapplying the filter after a rebuild keeps only matching widgets visible"
            );
        });
    });
}

#[test]
fn reapply_handles_multi_word_and_case() {
    // A multi-word, case-insensitive query survives the rebuild + reapply cycle.
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let mut page = stub_widgets_page();
            page.update_filter("AI INPUT", ctx);
            assert_eq!(visible_widget_count(&page), 1);

            let mut rebuilt = stub_widgets_page();
            rebuilt.update_filter("AI INPUT", ctx);
            assert_eq!(visible_widget_count(&rebuilt), 1);
        });
    });
}

#[test]
fn empty_query_after_reapply_shows_all_widgets() {
    // When the search is cleared, the subpage shows all widgets again.
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let mut page = stub_widgets_page();
            page.update_filter("agent", ctx);
            assert_eq!(visible_widget_count(&page), 1);

            let mut rebuilt = stub_widgets_page();
            rebuilt.update_filter("", ctx);
            assert_eq!(
                visible_widget_count(&rebuilt),
                5,
                "an empty query restores every widget on the subpage"
            );
        });
    });
}
