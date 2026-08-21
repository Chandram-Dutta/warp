/// A navigation key that an editor can propagate to its parent view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationKey {
    Tab,
    ShiftTab,
    Up,
    Down,
    PageUp,
    PageDown,
    Left,
    Right,
}
