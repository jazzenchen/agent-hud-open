use super::*;

fn view(screen: Value, layout: Value) -> Value {
    json!({"store": {"screens": {"a": screen}, "screenDefaults": {}}, "layouts": {"a": layout}, "sharedLayout": {}})
}

#[test]
fn a_screen_reads_its_own_value_then_the_defaults() {
    let shared = json!({"store": {"screens": {}, "screenDefaults": {"edge": "left"}}, "layouts": {}, "sharedLayout": {"style": {"width": 600}}});
    let screen = Screen::new(&shared, "a", false);
    assert!(screen.mode() == Mode::Simple);
    assert!(Screen::new(&shared, "a", true).mode() == Mode::Notch);
    assert_eq!(screen.text("edge"), "left");
    assert_eq!(screen.layout()["style"]["width"], 600);
    let own = view(
        json!({"mode": "simple", "offset": "far", "simpleSize": 30}),
        json!({"hud": {}}),
    );
    let screen = Screen::new(&own, "a", true);
    assert!(screen.mode() == Mode::Simple);
    // A value of the wrong type falls back to the default.
    assert_eq!(screen.number("offset"), 0.5);
    assert_eq!(screen.number("simpleSize"), 30.0);
    assert_eq!(screen.layout(), &json!({"hud": {}}));
    // A screen saved in the retired Dynamic mode draws as Simple.
    let dynamic = view(json!({"mode": "logos", "edge": "left"}), json!({}));
    let screen = Screen::new(&dynamic, "a", true);
    assert!(screen.mode() == Mode::Simple);
    assert_eq!(screen.mode_name(), "simple");
    assert_eq!(screen.text("edge"), "left");
}

#[test]
fn a_strip_is_as_wide_as_geometry_ts_draws_it() {
    let defaults = view(json!({"dockScale": 1}), json!({}));
    let screen = Screen::new(&defaults, "a", false);
    let labels = |n: usize| vec!["72%".to_owned(); n];
    assert_eq!(
        strip_width(&screen, Mode::Notch, Edge::Top, &labels(3), 185.0),
        185.0
    );
    assert_eq!(
        strip_width(&screen, Mode::Simple, Edge::Top, &[], 0.0),
        29.44
    );
    // Reduced vertical padding makes the leading inset 3 pt. The approved
    // whole dock scale is 92%; the edge's transparent 3 pt margin stays native.
    assert_eq!(
        strip_width(&screen, Mode::Simple, Edge::Top, &labels(3), 0.0),
        162.84
    );
    assert!(
        (strip_width(&screen, Mode::Simple, Edge::Right, &labels(3), 0.0) - 59.12).abs() < 0.001
    );
}

#[test]
fn compact_dock_ignores_full_mode_bars_for_its_window_estimate() {
    let saved = view(
        json!({"compactDock": true, "simpleStyle": "bars", "dockScale": 1}),
        json!({}),
    );
    let screen = Screen::new(&saved, "a", false);
    // One 24 pt mark, 2 pt label gap, conservative 25 pt label and 11 pt insets.
    let width = strip_width(&screen, Mode::Simple, Edge::Top, &["72%".into()], 0.0);
    assert!((width - 67.16).abs() < 0.001);
}

#[test]
fn whole_dock_size_and_vertical_thickness_match_the_drawing_scale() {
    let a = view(
        json!({"compactDock": true, "dockScale": 0.8, "dockHeight": 1}),
        json!({}),
    );
    let b = view(
        json!({"compactDock": true, "dockScale": 1.6, "dockHeight": 1.5}),
        json!({}),
    );
    let labels = vec!["72%".into()];
    let first = Screen::new(&a, "a", false);
    let second = Screen::new(&b, "a", false);
    let width = |screen, edge| strip_width(screen, Mode::Simple, edge, &labels, 0.0);
    assert!((width(&second, Edge::Top) / width(&first, Edge::Top) - 2.0).abs() < 0.001);
    assert!(
        ((width(&second, Edge::Right) - 3.0) / (width(&first, Edge::Right) - 3.0) - 3.0).abs()
            < 0.001
    );
}
