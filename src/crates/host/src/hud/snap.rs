//! Where a dropped HUD lands: the nearest edge of the display under the
//! pointer, and the offset along it that keeps the grip under the pointer.
//! On a screen with a notch, a drop near the notch lands in it as the Notch HUD,
//! and a drop anywhere else lands as a Simple dock.
use super::{
    global_units_per_point, host, Bounds, Display, DragState, Edge, HudPlacement, LogicalRect,
    Mode, Point,
};

/// How loosely a drop has to hit the notch, in points: this far to either side
/// of it, or this far below it, still lands in it.
const NOTCH_REACH_SIDE: f64 = 100.0;
const NOTCH_REACH_BELOW: f64 = 60.0;

pub(super) fn drop_placement(drag: &DragState, pointer: Point, display: &Display) -> HudPlacement {
    let monitor = &display.monitor;
    let position = monitor.position();
    let size = monitor.size();
    // Tao expresses macOS positions using each monitor's own backing scale. Convert
    // every display to one global point space before comparing it with NSEvent's cursor.
    let divisor = if host::GLOBAL_POINTS {
        monitor.scale_factor()
    } else {
        1.0
    };
    let bounds = Bounds {
        x: f64::from(position.x) / divisor,
        y: f64::from(position.y) / divisor,
        width: f64::from(size.width) / divisor,
        height: f64::from(size.height) / divisor,
    };
    let units = global_units_per_point(monitor.scale_factor());
    let near = display.measured.notch.is_some_and(|notch| {
        near_notch(
            (pointer.x - bounds.x) / units,
            (pointer.y - bounds.y) / units,
            &notch,
            &display.measured.screen,
        )
    });
    if near {
        return HudPlacement {
            screen_id: display.info.screen_id.clone(),
            mode: Mode::Notch,
            edge: Edge::Top,
            offset: 0.5,
            monitor_index: display.info.index,
        };
    }
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
    HudPlacement {
        screen_id: display.info.screen_id.clone(),
        mode: Mode::Simple,
        edge,
        offset,
        monitor_index: display.info.index,
    }
}

/// Whether a pointer `across` and `down` points from the screen's top-left
/// corner is at the notch, give or take the reach around it. Both rectangles
/// are in the same global points, so only their difference matters.
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
}
