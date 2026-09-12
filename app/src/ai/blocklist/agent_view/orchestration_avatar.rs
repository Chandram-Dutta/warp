use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use pathfinder_color::ColorU;
use warp_core::ui::color::coloru_with_opacity;
use warp_core::ui::theme::{Fill, WarpTheme};
use warpui::elements::{
    Align, ConstrainedBox, Container, CornerRadius, CrossAxisAlignment, Element, Empty, Flex,
    MainAxisSize, ParentElement, Radius, Stack, Text,
};
use warpui::fonts::{Properties, Weight};
use warpui::text_layout::ClipConfig;
use warpui::{AppContext, SingletonEntity};

use crate::appearance::Appearance;
use crate::ui_components::icons::Icon;

const TRANSCRIPT_AVATAR_SCALE: f32 = 1.25;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum OrchestrationAvatar {
    Orchestrator,
    Agent { display_name: String },
}

impl OrchestrationAvatar {
    pub(crate) fn agent(display_name: String) -> Self {
        Self::Agent { display_name }
    }

    pub(crate) fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let size = app.font_cache().line_height(
            appearance.monospace_font_size(),
            appearance.line_height_ratio(),
        ) * TRANSCRIPT_AVATAR_SCALE;

        match self {
            Self::Orchestrator => render_orchestrator_avatar_disc(size, theme, appearance),
            Self::Agent { display_name } => {
                render_agent_avatar_disc(display_name, size, theme, appearance)
            }
        }
    }
}

pub(crate) fn render_orchestrator_avatar_disc(
    size: f32,
    theme: &WarpTheme,
    appearance: &Appearance,
) -> Box<dyn Element> {
    render_avatar_disc(
        theme.ansi_fg_cyan(),
        AvatarGlyph::Icon(Icon::Agent),
        size,
        theme,
        appearance,
    )
}

pub(crate) fn render_agent_avatar_disc(
    name: &str,
    size: f32,
    theme: &WarpTheme,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let palette = [
        theme.ansi_fg_blue(),
        theme.ansi_fg_magenta(),
        theme.ansi_fg_cyan(),
        theme.ansi_fg_green(),
        theme.ansi_fg_yellow(),
        theme.ansi_fg_red(),
    ];
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let color = palette[(hasher.finish() as usize) % palette.len()];
    let initial = name
        .trim()
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('A');
    render_avatar_disc(color, AvatarGlyph::Letter(initial), size, theme, appearance)
}

enum AvatarGlyph {
    Letter(char),
    Icon(Icon),
}

fn render_avatar_disc(
    avatar_color: ColorU,
    glyph: AvatarGlyph,
    size: f32,
    theme: &WarpTheme,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let disc = ConstrainedBox::new(
        Container::new(Empty::new().finish())
            .with_background_color(avatar_color)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(size / 2.)))
            .finish(),
    )
    .with_width(size)
    .with_height(size)
    .finish();
    let glyph_size = size * 0.625;
    let glyph_element: Box<dyn Element> = match glyph {
        AvatarGlyph::Letter(letter) => {
            Text::new(letter.to_string(), appearance.ui_font_family(), glyph_size)
                .with_color(theme.background().into_solid())
                .with_style(Properties {
                    weight: Weight::Bold,
                    ..Default::default()
                })
                // Remove text leading so the glyph is centered in the disc.
                .with_line_height_ratio(1.)
                .finish()
        }
        AvatarGlyph::Icon(icon) => {
            ConstrainedBox::new(icon.to_warpui_icon(theme.background()).finish())
                .with_width(glyph_size)
                .with_height(glyph_size)
                .finish()
        }
    };
    let glyph_centered = ConstrainedBox::new(Align::new(glyph_element).finish())
        .with_width(size)
        .with_height(size)
        .finish();
    Stack::new()
        .with_child(disc)
        .with_child(glyph_centered)
        .finish()
}

pub(crate) fn render_static_agent_pill(name: &str, app: &AppContext) -> Box<dyn Element> {
    let appearance = Appearance::as_ref(app);
    let theme = appearance.theme();
    let avatar = render_agent_avatar_disc(name, 16., theme, appearance);
    let text_color = theme.ansi_fg_magenta();
    let label = Text::new(name.to_string(), appearance.ui_font_family(), 12.)
        .with_color(text_color)
        .soft_wrap(false)
        .with_clip(ClipConfig::ellipsis())
        .finish();
    let row = Flex::row()
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_main_axis_size(MainAxisSize::Min)
        .with_spacing(6.)
        .with_child(avatar)
        .with_child(ConstrainedBox::new(label).with_max_width(110.).finish())
        .finish();
    ConstrainedBox::new(
        Container::new(row)
            .with_padding_left(4.)
            .with_padding_right(10.)
            .with_background_color(coloru_with_opacity(text_color, 10))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(11.)))
            .finish(),
    )
    .with_height(22.)
    .finish()
}

#[cfg(test)]
#[path = "orchestration_avatar_tests.rs"]
mod tests;
