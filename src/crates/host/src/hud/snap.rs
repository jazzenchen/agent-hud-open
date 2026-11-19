//! Where a dropped HUD lands: the nearest edge of the display under the
//! pointer, and the offset along it that keeps the grip under the pointer.
//! On a screen with a notch, a dock that would land on the notch, give or take,
//! lands in it as the Notch HUD; anywhere else it lands as a Simple dock.
use super::{
    dock_bounds, global_units_per_point, host, Bounds, Display, DragState, Edge, HudPlacement,
    LogicalRect, Mode, Point,
};

/// How loosely a drop has to hit the notch, in points: the dock's middle this far
/// to either side of it, with the pointer no further than this below it.
const NOTCH_REACH_SIDE: f64 = 100.0;
const NOTCH_REACH_BELOW: f64 = 60.0;

pub(super) fn drop_placement(drag: &DragState, pointer: Point, display: &Display) -> HudPlacement {
    let monitor = &display.monitor;
    // The same area the HUD is placed in, so a drop beside the taskbar lands on that edge.
    let area = dock_bounds(monitor);
    // Tao expresses macOS positions using each monitor's own backing scale. Convert
    // every display to one global point space before comparing it with NSEvent's cursor.
    let divisor = if host::GLOBAL_POINTS {
        monitor.scale_factor()
    } else {
        1.0
    };
    let bounds = Bounds {
        x: f64::from(area.x) / divisor,
        y: f64::from(area.y) / divisor,
        width: f64::from(area.width) / divisor,
        height: f64::from(area.height) / divisor,
    };
    let units = global_units_per_point(monitor.scale_factor());
    let edge_distance = |edge: Edge| match edge {
        Edge::Top => (pointer.y - bounds.y).abs(),
        Edge::Right => (bounds.x + bounds.width - pointer.x).abs(),
        Edge::Bottom => (bounds.y + bounds.height - pointer.y).abs(),
        Edge::Left => (pointer.x - bounds.x).abs(),
    };
    // Start with the original edge, and use strict comparison just like NotchGeometry.
    // Exact diagonal ties retain that edge instead of oscillating between two placements.
    let mut edge = drag.edge;
    for candidate in [Edge::Top, Edge::Right, Edge::Bottom, Edge::Left] {
        if edge_distance(candidate) < edge_distance(edge) {
            edge = candidate;
        }
    }
    let length = if edge.horizontal() {
        bounds.width
    } else {
        bounds.height
    };
    let run = (if edge.horizontal() {
        drag.runs.horizontal
    } else {
        drag.runs.vertical
    } * units)
        .round()
        .min(length);
    let reading_coordinate = if edge.horizontal() {
        pointer.x - bounds.x
    } else {
        pointer.y - bounds.y
    };
    let travel = length - run;
    let offset = if travel > 0.0 {
        ((reading_coordinate - run * drag.grab_fraction) / travel).clamp(0.0, 1.0)
    } else {
        0.5
    };
    let dock = HudPlacement {
        screen_id: display.info.screen_id.clone(),
        mode: Mode::Simple,
        edge,
        offset,
        monitor_index: display.info.index,
    };
    // The dock's middle as it would rest on the top edge, not the pointer: a dock
    // resting beside the notch is often picked up right next to it.
    let at_notch = edge == Edge::Top
        && display.measured.notch.is_some_and(|notch| {
            near_notch(
                (travel.max(0.0) * offset + run / 2.0) / units,
                (pointer.y - bounds.y) / units,
                &notch,
                &display.measured.screen,
            )
        });
    if at_notch {
        HudPlacement {
            mode: Mode::Notch,
            offset: 0.5,
            ..dock
        }
    } else {
        dock
    }
}

/// Whether a top-edge dock whose middle is `across` points from the screen's
/// left edge, dragged with the pointer `down` points below the top, is at the
/// notch, give or take the reach around it. Both rectangles are in the same
/// global points, so only their difference matters.
fn near_notch(across: f64, down: f64, notch: &LogicalRect, screen: &LogicalRect) -> bool {
    let left = notch.x - screen.x;
    let right = left + notch.width;
    let bottom = notch.y - screen.y + notch.height;
    (left - NOTCH_REACH_SIDE..=right + NOTCH_REACH_SIDE).contains(&across)
        && (-NOTCH_REACH_BELOW..=bottom + NOTCH_REACH_BELOW).contains(&down)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 14-inch MacBook Pro's screen and notch, as AppKit measures them, on
    /// a display that sits to the right of another one.
    fn screen() -> LogicalRect {
        LogicalRect {
            x: 1920.0,
            y: 0.0,
            width: 1512.0,
            height: 982.0,
        }
    }

    fn notch() -> LogicalRect {
        LogicalRect {
            x: 1920.0 + 656.0,
            y: 0.0,
            width: 200.0,
            height: 32.0,
        }
    }

    #[test]
    fn a_drop_on_the_notch_or_close_to_it_lands_in_it() {
        assert!(near_notch(756.0, 10.0, &notch(), &screen()));
        // Off to one side and a little below, not aimed precisely.
        assert!(near_notch(656.0 - 90.0, 32.0 + 50.0, &notch(), &screen()));
        assert!(near_notch(856.0 + 90.0, 0.0, &notch(), &screen()));
    }

    #[test]
    fn a_drop_well_away_from_the_notch_stays_a_dock() {
        assert!(!near_notch(656.0 - 120.0, 10.0, &notch(), &screen()));
        assert!(!near_notch(856.0 + 120.0, 10.0, &notch(), &screen()));
        assert!(!near_notch(756.0, 32.0 + 80.0, &notch(), &screen()));
        assert!(!near_notch(20.0, 10.0, &notch(), &screen()));
    }

    #[test]
    fn a_dock_resting_just_beside_the_notch_stays_a_dock_when_picked_up() {
        // A 16-inch MacBook Pro at 2056 points: a 294-point dock that ends
        // 6 points left of the notch has its middle at 765.
        let screen = LogicalRect {
            x: 0.0,
            y: 0.0,
            width: 2056.0,
            height: 1329.0,
        };
        let notch = LogicalRect {
            x: 918.0,
            y: 0.0,
            width: 220.0,
            height: 38.0,
        };
        assert!(!near_notch(765.0, 14.0, &notch, &screen));
        assert!(near_notch(765.0 + 60.0, 14.0, &notch, &screen));
    }
}
