use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use super::window_state::{WindowGeometry, WindowPlacement};
use crate::runtime::saveable::persist;

const GEOMETRY_VERSION: u64 = 1;
const MAX_LOGICAL_EXTENT: f32 = 65_536.0;
const MAX_LOGICAL_COORDINATE: f32 = 1_000_000.0;
const MAX_SCALE_FACTOR: f32 = 32.0;

static MEMORY: LazyLock<Mutex<HashMap<u64, WindowGeometry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Physical monitor bounds used to keep restored windows reachable.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MonitorBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f64,
}

impl MonitorBounds {
    pub(crate) fn from_handle(handle: &winit::monitor::MonitorHandle) -> Option<Self> {
        let position = handle.position()?;
        let size = handle.current_video_mode()?.size();
        let scale_factor = handle.scale_factor();
        if size.width == 0 || size.height == 0 || !scale_factor.is_finite() || scale_factor <= 0.0 {
            return None;
        }
        Some(Self {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
            scale_factor,
        })
    }
}

/// Save normal-window geometry in memory and, when configured, in the keyed PersistStore file.
pub(crate) fn save(key: u64, geometry: WindowGeometry) {
    let Some(geometry) = normalized(geometry) else {
        return;
    };
    MEMORY
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(key, geometry.clone());
    if let Some(store) = persist::store()
        && let Ok(value) = serde_json::to_value(&geometry)
    {
        store.save_extra(key, "geometry", value);
    }
}

/// Load the in-process value first so close/reopen works without a configured disk store.
pub(crate) fn load(key: u64) -> Option<WindowGeometry> {
    if let Some(geometry) = MEMORY
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&key)
        .cloned()
    {
        return normalized(geometry);
    }
    let value = persist::store()?.load_extra(key, "geometry")?;
    normalized(serde_json::from_value(value).ok()?)
}

/// Validate saved geometry and clamp it to the current monitor bounds when those are available.
pub(crate) fn restore(key: u64, monitors: &[MonitorBounds]) -> Option<WindowGeometry> {
    let geometry = load(key)?;
    Some(clamp_to_monitors(geometry, monitors))
}

fn normalized(mut geometry: WindowGeometry) -> Option<WindowGeometry> {
    if geometry.version != GEOMETRY_VERSION
        || !geometry.width.is_finite()
        || !geometry.height.is_finite()
        || geometry.width <= 0.0
        || geometry.height <= 0.0
        || geometry.width > MAX_LOGICAL_EXTENT
        || geometry.height > MAX_LOGICAL_EXTENT
        || !geometry.scale_factor.is_finite()
        || geometry.scale_factor <= 0.0
        || geometry.scale_factor > MAX_SCALE_FACTOR
    {
        return None;
    }
    if let Some(position) = geometry.position
        && (!position.x.is_finite()
            || !position.y.is_finite()
            || position.x.abs() > MAX_LOGICAL_COORDINATE
            || position.y.abs() > MAX_LOGICAL_COORDINATE)
    {
        return None;
    }
    if geometry.placement == WindowPlacement::Minimized {
        geometry.placement = WindowPlacement::Windowed;
    }
    Some(geometry)
}

fn clamp_to_monitors(mut geometry: WindowGeometry, monitors: &[MonitorBounds]) -> WindowGeometry {
    if monitors.is_empty() {
        return geometry;
    }

    let saved_scale = geometry.scale_factor as f64;
    let saved_width = geometry.width as f64 * saved_scale;
    let saved_height = geometry.height as f64 * saved_scale;
    let saved_position = geometry.position.map(|position| {
        (
            (position.x as f64 * saved_scale).round() as i64,
            (position.y as f64 * saved_scale).round() as i64,
        )
    });

    let mut best: Option<(i64, usize)> = None;
    for (index, monitor) in monitors.iter().enumerate() {
        let Some((left, top)) = saved_position else {
            continue;
        };
        let right = left + saved_width.round() as i64;
        let bottom = top + saved_height.round() as i64;
        let monitor_right = monitor.x as i64 + monitor.width as i64;
        let monitor_bottom = monitor.y as i64 + monitor.height as i64;
        let intersection = (right.min(monitor_right) - left.max(monitor.x as i64)).max(0)
            * (bottom.min(monitor_bottom) - top.max(monitor.y as i64)).max(0);
        if intersection > 0 && best.is_none_or(|(area, _)| intersection > area) {
            best = Some((intersection, index));
        }
    }

    let target = best.map(|(_, index)| monitors[index]).unwrap_or_else(|| {
        monitors
            .iter()
            .max_by_key(|monitor| monitor.width as u64 * monitor.height as u64)
            .copied()
            .unwrap_or(monitors[0])
    });
    geometry.scale_factor = target.scale_factor as f32;
    let max_width = (target.width as f64 / target.scale_factor) as f32;
    let max_height = (target.height as f64 / target.scale_factor) as f32;
    geometry.width = geometry.width.min(max_width).max(1.0);
    geometry.height = geometry.height.min(max_height).max(1.0);

    if let Some((left, top)) = saved_position {
        if best.is_some() {
            let width = (geometry.width as f64 * target.scale_factor).round() as i64;
            let height = (geometry.height as f64 * target.scale_factor).round() as i64;
            let max_left = target.x as i64 + target.width as i64 - width;
            let max_top = target.y as i64 + target.height as i64 - height;
            let left = left.clamp(target.x as i64, max_left.max(target.x as i64));
            let top = top.clamp(target.y as i64, max_top.max(target.y as i64));
            geometry.position = Some(super::window_state::GeometryPosition {
                x: (left as f64 / target.scale_factor) as f32,
                y: (top as f64 / target.scale_factor) as f32,
            });
        } else {
            geometry.position = None;
        }
    }
    geometry
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> WindowGeometry {
        WindowGeometry {
            version: GEOMETRY_VERSION,
            width: 640.0,
            height: 480.0,
            position: Some(super::super::window_state::GeometryPosition { x: 120.0, y: 80.0 }),
            placement: WindowPlacement::Windowed,
            scale_factor: 1.0,
        }
    }

    fn monitor(x: i32, y: i32, width: u32, height: u32, scale_factor: f64) -> MonitorBounds {
        MonitorBounds {
            x,
            y,
            width,
            height,
            scale_factor,
        }
    }

    #[test]
    fn geometry_round_trips_through_json() {
        let value = serde_json::to_value(geometry()).unwrap();
        let decoded: WindowGeometry = serde_json::from_value(value).unwrap();
        assert_eq!(decoded, geometry());
    }

    #[test]
    fn invalid_geometry_is_rejected() {
        let mut wrong_version = geometry();
        wrong_version.version = 99;
        assert!(normalized(wrong_version).is_none());
        let mut invalid_size = geometry();
        invalid_size.width = f32::NAN;
        assert!(normalized(invalid_size).is_none());
        let mut invalid_scale = geometry();
        invalid_scale.scale_factor = 0.0;
        assert!(normalized(invalid_scale).is_none());
    }

    #[test]
    fn minimized_geometry_restores_as_windowed() {
        let mut saved = geometry();
        saved.placement = WindowPlacement::Minimized;
        assert_eq!(
            normalized(saved).unwrap().placement,
            WindowPlacement::Windowed
        );
    }

    #[test]
    fn memory_store_round_trips_and_normalizes_minimized() {
        let key = 0x4759_0001;
        let mut saved = geometry();
        saved.placement = WindowPlacement::Minimized;
        save(key, saved);
        let restored = load(key).unwrap();
        assert_eq!(restored.placement, WindowPlacement::Windowed);
        assert_eq!(restored.width, geometry().width);
    }

    #[test]
    fn offscreen_geometry_falls_back_to_platform_position() {
        let mut saved = geometry();
        saved.position = Some(super::super::window_state::GeometryPosition {
            x: 5_000.0,
            y: 5_000.0,
        });
        let restored = clamp_to_monitors(saved, &[monitor(0, 0, 1920, 1080, 1.0)]);
        assert_eq!(restored.position, None);
    }

    #[test]
    fn partially_offscreen_geometry_is_clamped() {
        let mut saved = geometry();
        saved.position = Some(super::super::window_state::GeometryPosition {
            x: 1_900.0,
            y: 100.0,
        });
        let restored = clamp_to_monitors(saved, &[monitor(0, 0, 1920, 1080, 1.0)]);
        assert_eq!(restored.position.unwrap().x, 1280.0);
    }

    #[test]
    fn dpi_change_reinterprets_position_in_target_logical_space() {
        let mut saved = geometry();
        saved.width = 200.0;
        saved.height = 150.0;
        saved.position = Some(super::super::window_state::GeometryPosition {
            x: 1_500.0,
            y: 100.0,
        });
        let restored = clamp_to_monitors(saved, &[monitor(0, 0, 1920, 1080, 2.0)]);
        assert_eq!(restored.position.unwrap().x, 750.0);
        assert_eq!(restored.scale_factor, 2.0);
    }
}
