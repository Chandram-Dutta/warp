use warpui::elements::{Container, Element, Empty};
use warpui::keymap::Keystroke;
use warpui::{AppContext, Entity, ModelHandle, View, ViewContext};

use super::message_bar::common::render_terminal_message;
use super::message_bar::{Message, MessageItem};
use crate::terminal::input::inline_history::AcceptHistoryItem;
use crate::terminal::input::inline_menu::{InlineMenuModel, InlineMenuModelEvent};
use crate::terminal::input::suggestions_mode_model::{
    InputSuggestionsModeEvent, InputSuggestionsModeModel,
};

pub struct TerminalInputMessageBar {
    suggestions_mode_model: ModelHandle<InputSuggestionsModeModel>,
    inline_history_model: ModelHandle<InlineMenuModel<AcceptHistoryItem>>,
}

impl Entity for TerminalInputMessageBar {
    type Event = ();
}

impl TerminalInputMessageBar {
    pub fn new(
        suggestions_mode_model: ModelHandle<InputSuggestionsModeModel>,
        inline_history_model: ModelHandle<InlineMenuModel<AcceptHistoryItem>>,
        ctx: &mut ViewContext<Self>,
    ) -> Self {
        ctx.subscribe_to_model(&suggestions_mode_model, |_, _, event, ctx| {
            let InputSuggestionsModeEvent::ModeChanged { .. } = event;
            ctx.notify();
        });
        ctx.subscribe_to_model(&inline_history_model, |_, _, event, ctx| {
            if let InlineMenuModelEvent::UpdatedSelectedItem = event {
                ctx.notify();
            }
        });
        Self {
            suggestions_mode_model,
            inline_history_model,
        }
    }
}

impl View for TerminalInputMessageBar {
    fn ui_name() -> &'static str {
        "TerminalInputMessageBar"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        if !self
            .suggestions_mode_model
            .as_ref(app)
            .is_inline_history_menu()
        {
            return Empty::new().finish();
        }
        let enter = MessageItem::keystroke(Keystroke {
            key: "enter".to_owned(),
            ..Default::default()
        });
        let items = match self.inline_history_model.as_ref(app).selected_item() {
            Some(_) => {
                vec![enter, MessageItem::text(" to execute")]
            }
            None => vec![],
        };
        Container::new(render_terminal_message(Message::new(items), app))
            .with_padding_bottom(8.)
            .with_padding_right(8.)
            .finish()
    }
}
