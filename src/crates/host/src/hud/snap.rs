//! Where a dropped HUD lands: the nearest edge of the display under the
//! pointer, and the offset along it that keeps the grip under the pointer.
use super::{global_units_per_point, host, Bounds, Display, DragState, Edge, HudPlacement, Point};

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
    } * global_units_per_point(monitor.scale_factor()))
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
        mode: drag.mode,
        edge,
        offset,
        monitor_index: display.info.index,
    }
}
