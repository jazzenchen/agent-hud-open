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
    let defaults = view(json!({}), json!({}));
    let screen = Screen::new(&defaults, "a", false);
    let labels = |n: usize| vec!["72%".to_owned(); n];
    assert_eq!(
        strip_width(&screen, Mode::Notch, Edge::Top, &labels(3), 185.0),
        185.0
    );
    assert_eq!(
        strip_width(&screen, Mode::Simple, Edge::Top, &[], 0.0),
        32.0
    );
    // Simple, 22 pt with rings and numbers: 5 + 3 × (22 + 5 + 21) + 2 × 10 + 10,
    // or a column as wide as its widest row and 3 pt of margin.
    assert_eq!(
        strip_width(&screen, Mode::Simple, Edge::Top, &labels(3), 0.0),
        179.0
    );
    assert_eq!(
        strip_width(&screen, Mode::Simple, Edge::Right, &labels(3), 0.0),
        66.0
    );
}

#[test]
fn a_screen_reads_its_own_hide_switch() {
    let view = view(json!({"hidden": true}), json!({}));
    assert!(Screen::new(&view, "a", false).flag("hidden"));
    assert!(!Screen::new(&view, "b", false).flag("hidden"));
}

#[test]
fn shortest_window_first_counts_only_on_a_simple_dock_with_numbers() {
    let on = |own: Value, has_notch: bool| {
        let view = view(own, json!({}));
        Screen::new(&view, "a", has_notch).shortest_window_first()
    };
    // The defaults draw segments with numbers beside them.
    assert!(on(
        json!({"mode": "simple", "shortestWindowFirst": true}),
        false
    ));
    assert!(!on(json!({"mode": "simple"}), false));
    // A Notch writes no numbers, and a dock without them has none to move.
    assert!(!on(
        json!({"mode": "notch", "shortestWindowFirst": true}),
        true
    ));
    assert!(!on(json!({"shortestWindowFirst": true}), true));
    assert!(!on(
        json!({"mode": "simple", "showsNumbers": false, "shortestWindowFirst": true}),
        false
    ));
    assert!(on(
        json!({"mode": "simple", "simpleStyle": "numbers", "showsNumbers": false,
            "shortestWindowFirst": true}),
        false
    ));
}
