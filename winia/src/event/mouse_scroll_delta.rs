#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseScrollDelta {
    LineDelta(f32, f32),
    Delta(f32, f32),
}

impl MouseScrollDelta {
    pub fn from_winit(delta: winit::event::MouseScrollDelta, scale_factor: f32) -> Self {
        match delta {
            winit::event::MouseScrollDelta::LineDelta(x, y) => MouseScrollDelta::LineDelta(x, y),
            winit::event::MouseScrollDelta::PixelDelta(pos) => {
                let logical_pos = pos.to_logical(scale_factor as f64);
                MouseScrollDelta::Delta(logical_pos.x, logical_pos.y)
            }
        }
    }

    pub fn is_x_scrollable(&self) -> bool {
        matches!(self, MouseScrollDelta::LineDelta(x, _) if *x != 0.0)
            || matches!(self, MouseScrollDelta::Delta(x, _) if *x != 0.0)
    }

    pub fn is_y_scrollable(&self) -> bool {
        matches!(self, MouseScrollDelta::LineDelta(_, y) if *y != 0.0)
            || matches!(self, MouseScrollDelta::Delta(_, y) if *y != 0.0)
    }

    pub fn is_scrollable(&self) -> bool {
        self.is_x_scrollable() || self.is_y_scrollable()
    }
    
    pub fn disable_x(&mut self) {
        if let MouseScrollDelta::LineDelta(_, y) = self {
            *y = 0.0;
        }
        if let MouseScrollDelta::Delta(_, y) = self {
            *y = 0.0;
        }
    }
    pub fn disable_y(&mut self) {
        if let MouseScrollDelta::LineDelta(_, y) = self {
            *y = 0.0;
        }
        if let MouseScrollDelta::Delta(_, y) = self {
            *y = 0.0;
        }
    }
}