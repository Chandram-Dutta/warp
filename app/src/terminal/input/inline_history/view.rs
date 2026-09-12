//! Inline command history menu.
use warpui::elements::ChildView;
use warpui::{AppContext, Element, Entity, ModelHandle, View, ViewContext, ViewHandle};

use crate::features::FeatureFlag;
use crate::search::data_source::{Query, QueryFilter};
use crate::search::mixer::SearchMixer;
use crate::terminal::history::LinkedWorkflowData;
use crate::terminal::input::buffer_model::{InputBufferModel, InputBufferUpdateEvent};
use crate::terminal::input::inline_history::data_source::{
    AcceptHistoryItem, InlineHistoryMenuDataSource,
};
use crate::terminal::input::inline_menu::{
    InlineMenuEvent, InlineMenuHeaderConfig, InlineMenuModel, InlineMenuPositioner, InlineMenuView,
};
use crate::terminal::input::suggestions_mode_model::{
    InputSuggestionsModeEvent, InputSuggestionsModeModel,
};
use crate::terminal::model::session::active_session::ActiveSession;

#[derive(Debug, Clone)]
pub enum InlineHistoryMenuEvent {
    AcceptCommand {
        command: String,
    },
    SelectCommand {
        command: String,
        linked_workflow_data: Option<LinkedWorkflowData>,
    },
    /// Close the menu and restore the original input buffer.
    Close,
}

pub struct InlineHistoryMenuView {
    menu_view: ViewHandle<InlineMenuView<AcceptHistoryItem>>,
    mixer: ModelHandle<SearchMixer<AcceptHistoryItem>>,
    model: ModelHandle<InlineMenuModel<AcceptHistoryItem>>,
    buffer_model: ModelHandle<InputBufferModel>,
    pending_initial_buffer_sync: bool,
}

impl InlineHistoryMenuView {
    pub fn new(
        active_session: ModelHandle<ActiveSession>,
        input_suggestions_model: &ModelHandle<InputSuggestionsModeModel>,
        positioner: &ModelHandle<InlineMenuPositioner>,
        buffer_model: ModelHandle<InputBufferModel>,
        ctx: &mut ViewContext<Self>,
    ) -> Self {
        let data_source = ctx.add_model(|_| InlineHistoryMenuDataSource::new(active_session));
        let mixer = ctx.add_model(|ctx| {
            let mut mixer = SearchMixer::<AcceptHistoryItem>::new();
            mixer.add_sync_source(data_source, [QueryFilter::Commands]);
            mixer.run_query(
                Query {
                    text: String::new(),
                    filters: Default::default(),
                },
                ctx,
            );
            mixer
        });
        let menu_view = ctx.add_typed_action_view(|ctx| {
            let view = InlineMenuView::new(
                mixer.clone(),
                positioner.clone(),
                input_suggestions_model,
                ctx,
            );
            if FeatureFlag::InlineMenuHeaders.is_enabled() {
                view.with_header_config(InlineMenuHeaderConfig {
                    label: "History".to_owned(),
                    trailing_element: None,
                })
            } else {
                view
            }
        });
        let model = menu_view.as_ref(ctx).model().clone();
        ctx.subscribe_to_model(input_suggestions_model, |me, model, event, ctx| {
            let InputSuggestionsModeEvent::ModeChanged { .. } = event;
            if model.as_ref(ctx).is_inline_history_menu() {
                me.open_with_current_buffer(ctx);
            }
        });
        let suggestions_mode_model = input_suggestions_model.clone();
        ctx.subscribe_to_model(
            &buffer_model,
            move |me, _, _: &InputBufferUpdateEvent, ctx| {
                if suggestions_mode_model.as_ref(ctx).is_inline_history_menu()
                    && me.pending_initial_buffer_sync
                {
                    me.pending_initial_buffer_sync = false;
                    me.open_with_current_buffer(ctx);
                }
            },
        );
        ctx.subscribe_to_view(&menu_view, |_, _, event, ctx| match event {
            InlineMenuEvent::AcceptedItem { item, .. } => {
                ctx.emit(InlineHistoryMenuEvent::AcceptCommand {
                    command: item.command.clone(),
                })
            }
            InlineMenuEvent::SelectedItem { item } => {
                ctx.emit(InlineHistoryMenuEvent::SelectCommand {
                    command: item.command.clone(),
                    linked_workflow_data: item.linked_workflow_data.clone(),
                })
            }
            InlineMenuEvent::Dismissed => ctx.emit(InlineHistoryMenuEvent::Close),
            InlineMenuEvent::NoResults | InlineMenuEvent::TabChanged => {}
        });
        Self {
            menu_view,
            mixer,
            model,
            buffer_model,
            pending_initial_buffer_sync: false,
        }
    }

    pub fn model(&self) -> &ModelHandle<InlineMenuModel<AcceptHistoryItem>> {
        &self.model
    }

    pub fn render_results_only(&self, app: &AppContext) -> Box<dyn Element> {
        self.menu_view
            .as_ref(app)
            .render_results_only(false, 0., app)
    }

    pub fn result_count(&self, app: &AppContext) -> usize {
        self.menu_view.as_ref(app).result_count()
    }

    pub fn select_up(&self, ctx: &mut ViewContext<Self>) {
        self.menu_view.update(ctx, |v, ctx| v.select_up(ctx));
    }

    pub fn select_down(&self, ctx: &mut ViewContext<Self>) {
        let should_close = self.menu_view.read(ctx, |v, _| {
            let count = v.result_count();
            count == 0 || v.selected_idx().is_some_and(|idx| idx == count - 1)
        });
        if should_close {
            ctx.emit(InlineHistoryMenuEvent::Close);
        } else {
            self.menu_view.update(ctx, |v, ctx| v.select_down(ctx));
        }
    }

    pub fn accept_selected_item(&self, ctx: &mut ViewContext<Self>) {
        self.menu_view
            .update(ctx, |v, ctx| v.accept_selected_item(false, ctx));
    }

    pub fn arm_initial_buffer_sync(&mut self) {
        self.pending_initial_buffer_sync = true;
    }

    fn open_with_current_buffer(&mut self, ctx: &mut ViewContext<Self>) {
        let text = self.buffer_model.as_ref(ctx).current_value().to_owned();
        self.mixer.update(ctx, |mixer, ctx| {
            mixer.run_query(
                Query {
                    text,
                    filters: Default::default(),
                },
                ctx,
            );
        });
    }
}

impl View for InlineHistoryMenuView {
    fn ui_name() -> &'static str {
        "InlineHistoryMenuView"
    }

    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        ChildView::new(&self.menu_view).finish()
    }
}

impl Entity for InlineHistoryMenuView {
    type Event = InlineHistoryMenuEvent;
}
