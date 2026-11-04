use super::*;

fn view(screen: Value, layout: Value) -> Value {
    json!({"store": {"screens": {"a": screen}, "screenDefaults": {}}, "layouts": {"a": layout}, "sharedLayout": {}})
}

#[test]
fn a_screen_reads_its_own_value_then_the_defaults() {
    let shared = json!({"store": {"screens": {}, "screenDefaults": {"edge": "left"}}, "layouts": {}, "sharedLayout": {"style": {"width": 600}}});
    let screen = Screen::new(&shared, "a", false);
    assert!(screen.mode() == Mode::Logos);
    assert!(Screen::new(&shared, "a", true).mode() == Mode::Notch);
    assert_eq!(screen.text("edge"), "left");
    assert_eq!(screen.layout()["style"]["width"], 600);
    let own = view(
        json!({"mode": "simple", "offset": "far", "logoSize": 30}),
        json!({"hud": {}}),
    );
    let screen = Screen::new(&own, "a", true);
    assert!(screen.mode() == Mode::Simple);
    // A value of the wrong type falls back to the default.
    assert_eq!(screen.number("offset"), 0.5);
    assert_eq!(screen.number("logoSize"), 30.0);
    assert_eq!(screen.layout(), &json!({"hud": {}}));
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
    // Logos: 20 pt marks 8 pt apart, or one 24 pt column on a side.
    assert_eq!(
        strip_width(&screen, Mode::Logos, Edge::Top, &labels(3), 0.0),
        76.0
    );
    assert_eq!(strip_width(&screen, Mode::Logos, Edge::Top, &[], 0.0), 32.0);
    assert_eq!(
        strip_width(&screen, Mode::Logos, Edge::Left, &labels(3), 0.0),
        24.0
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
