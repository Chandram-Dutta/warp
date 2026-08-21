use std::cell::RefCell;
use std::collections::HashMap;

use settings::Setting as _;
use warp_errors::report_if_error;
use warp_local_ai::LocalAIProvider;
use warpui::elements::{ChildView, ConstrainedBox, Element, MouseStateHandle};
use warpui::ui_components::components::{Coords, UiComponent as _, UiComponentStyles};
use warpui::{AppContext, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle};

use super::settings_page::{
    LocalOnlyIconState, MatchData, PageType, SettingsPageMeta, SettingsPageViewHandle,
    SettingsWidget, render_body_item,
};
use super::{SettingsSection, ToggleState};
use crate::appearance::Appearance;
use crate::editor::{
    EditorView, Event as EditorEvent, PropagateAndNoOpNavigationKeys, SingleLineEditorOptions,
    TextColors, TextOptions,
};
use crate::settings::{LocalAICredentials, LocalAINextCommandSetting, LocalAISettings};
use crate::view_components::{Dropdown, DropdownItem};

#[derive(Clone, Debug, PartialEq)]
pub enum LocalAISettingsPageAction {
    SetEnabled(bool),
    SetProvider(LocalAIProvider),
}

pub struct LocalAISettingsPageView {
    page: PageType<Self>,
    local_only_icon_tooltip_states: RefCell<HashMap<String, MouseStateHandle>>,
    enabled_dropdown: ViewHandle<Dropdown<LocalAISettingsPageAction>>,
    provider_dropdown: ViewHandle<Dropdown<LocalAISettingsPageAction>>,
    endpoint_editor: ViewHandle<EditorView>,
    model_editor: ViewHandle<EditorView>,
    credential_editor: ViewHandle<EditorView>,
}

impl LocalAISettingsPageView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let enabled_dropdown = ctx.add_typed_action_view(|ctx| {
            let mut dropdown = Dropdown::new(ctx);
            dropdown.set_top_bar_max_width(360.);
            dropdown
        });
        let provider_dropdown = ctx.add_typed_action_view(|ctx| {
            let mut dropdown = Dropdown::new(ctx);
            dropdown.set_top_bar_max_width(360.);
            dropdown
        });
        let endpoint_editor = Self::create_editor(false, ctx);
        let model_editor = Self::create_editor(false, ctx);
        let credential_editor = Self::create_editor(true, ctx);

        ctx.subscribe_to_view(&endpoint_editor, |view, editor, event, ctx| {
            if matches!(event, EditorEvent::Blurred | EditorEvent::Enter) {
                let value = editor.as_ref(ctx).buffer_text(ctx).trim().to_owned();
                view.save_endpoint(value, ctx);
            }
        });
        ctx.subscribe_to_view(&model_editor, |view, editor, event, ctx| {
            if matches!(event, EditorEvent::Blurred | EditorEvent::Enter) {
                let value = editor.as_ref(ctx).buffer_text(ctx).trim().to_owned();
                view.save_model(value, ctx);
            }
        });
        ctx.subscribe_to_view(&credential_editor, |view, editor, event, ctx| {
            if matches!(event, EditorEvent::Blurred | EditorEvent::Enter) {
                let value = editor.as_ref(ctx).buffer_text(ctx);
                view.save_credential((!value.trim().is_empty()).then_some(value), ctx);
            }
        });
        ctx.subscribe_to_model(&LocalAISettings::handle(ctx), |view, _, _, ctx| {
            view.refresh_controls(ctx);
            ctx.notify();
        });
        ctx.subscribe_to_model(&LocalAICredentials::handle(ctx), |view, _, _, ctx| {
            view.refresh_credential(ctx);
            ctx.notify();
        });

        let view = Self {
            page: PageType::new_uncategorized(
                vec![
                    Box::new(NextCommandEnabledWidget),
                    Box::new(ProviderWidget),
                    Box::new(EndpointWidget),
                    Box::new(ModelWidget),
                    Box::new(CredentialWidget),
                ],
                Some("Next Command AI"),
            ),
            local_only_icon_tooltip_states: RefCell::new(HashMap::new()),
            enabled_dropdown,
            provider_dropdown,
            endpoint_editor,
            model_editor,
            credential_editor,
        };
        view.refresh_controls(ctx);
        view
    }

    fn create_editor(is_password: bool, ctx: &mut ViewContext<Self>) -> ViewHandle<EditorView> {
        ctx.add_typed_action_view(move |ctx| {
            let appearance = Appearance::as_ref(ctx);
            let options = SingleLineEditorOptions {
                is_password,
                propagate_and_no_op_vertical_navigation_keys:
                    PropagateAndNoOpNavigationKeys::Always,
                text: TextOptions {
                    font_size_override: Some(appearance.ui_font_size()),
                    font_family_override: Some(appearance.monospace_font_family()),
                    text_colors_override: Some(TextColors {
                        default_color: appearance.theme().active_ui_text_color(),
                        disabled_color: appearance.theme().disabled_ui_text_color(),
                        hint_color: appearance.theme().disabled_ui_text_color(),
                    }),
                    ..Default::default()
                },
                ..Default::default()
            };
            EditorView::single_line(options, ctx)
        })
    }

    fn active_provider(ctx: &AppContext) -> LocalAIProvider {
        LocalAISettings::as_ref(ctx).next_command.active_provider
    }

    fn save_endpoint(&self, endpoint: String, ctx: &mut ViewContext<Self>) {
        let provider = Self::active_provider(ctx);
        LocalAISettings::handle(ctx).update(ctx, |settings, ctx| {
            let mut config = settings.next_command.value().clone();
            config.provider_mut(provider).endpoint = endpoint;
            report_if_error!(settings.next_command.set_value(config, ctx));
        });
    }

    fn save_model(&self, model: String, ctx: &mut ViewContext<Self>) {
        let provider = Self::active_provider(ctx);
        LocalAISettings::handle(ctx).update(ctx, |settings, ctx| {
            let mut config = settings.next_command.value().clone();
            config.provider_mut(provider).model = model;
            report_if_error!(settings.next_command.set_value(config, ctx));
        });
    }

    fn save_credential(&self, credential: Option<String>, ctx: &mut ViewContext<Self>) {
        let provider = Self::active_provider(ctx);
        let result = LocalAICredentials::handle(ctx).update(ctx, |credentials, ctx| {
            credentials.set_credential(provider, credential, ctx)
        });
        if result.is_err() {
            log::error!("Failed to store local AI provider credential in secure storage");
        }
    }

    fn refresh_controls(&self, ctx: &mut ViewContext<Self>) {
        let config = LocalAISettings::as_ref(ctx).next_command.value().clone();
        self.enabled_dropdown.update(ctx, |dropdown, ctx| {
            dropdown.set_items(
                vec![
                    DropdownItem::new("Enabled", LocalAISettingsPageAction::SetEnabled(true)),
                    DropdownItem::new("Disabled", LocalAISettingsPageAction::SetEnabled(false)),
                ],
                ctx,
            );
            dropdown
                .set_selected_by_action(LocalAISettingsPageAction::SetEnabled(config.enabled), ctx);
        });
        self.provider_dropdown.update(ctx, |dropdown, ctx| {
            dropdown.set_items(
                LocalAIProvider::ALL
                    .into_iter()
                    .map(|provider| {
                        DropdownItem::new(
                            provider.display_name(),
                            LocalAISettingsPageAction::SetProvider(provider),
                        )
                    })
                    .collect(),
                ctx,
            );
            dropdown.set_selected_by_action(
                LocalAISettingsPageAction::SetProvider(config.active_provider),
                ctx,
            );
        });
        let provider_config = config.provider(config.active_provider);
        self.endpoint_editor.update(ctx, |editor, ctx| {
            editor.system_reset_buffer_text(&provider_config.endpoint, ctx);
            editor.set_placeholder_text("Provider base URL", ctx);
        });
        self.model_editor.update(ctx, |editor, ctx| {
            editor.system_reset_buffer_text(&provider_config.model, ctx);
            editor.set_placeholder_text("Provider model name", ctx);
        });
        self.refresh_credential(ctx);
    }

    fn refresh_credential(&self, ctx: &mut ViewContext<Self>) {
        let provider = Self::active_provider(ctx);
        let credential = LocalAICredentials::as_ref(ctx)
            .credential(provider)
            .unwrap_or_default()
            .to_owned();
        self.credential_editor.update(ctx, |editor, ctx| {
            editor.system_reset_buffer_text(&credential, ctx);
            editor.set_placeholder_text(
                if provider.requires_api_key() {
                    "Required API key"
                } else {
                    "Optional API key"
                },
                ctx,
            );
        });
    }

    fn render_editor(
        &self,
        editor: ViewHandle<EditorView>,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        ConstrainedBox::new(
            appearance
                .ui_builder()
                .text_input(editor)
                .with_style(UiComponentStyles {
                    padding: Some(Coords::uniform(8.)),
                    background: Some(appearance.theme().surface_2().into()),
                    ..Default::default()
                })
                .build()
                .finish(),
        )
        .with_width(360.)
        .finish()
    }

    fn local_only_icon(&self, key: &str, app: &AppContext) -> LocalOnlyIconState {
        LocalOnlyIconState::for_setting(
            key,
            LocalAINextCommandSetting::sync_to_cloud(),
            &mut self.local_only_icon_tooltip_states.borrow_mut(),
            app,
        )
    }
}

impl Entity for LocalAISettingsPageView {
    type Event = ();
}

impl TypedActionView for LocalAISettingsPageView {
    type Action = LocalAISettingsPageAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            LocalAISettingsPageAction::SetEnabled(enabled) => {
                LocalAISettings::handle(ctx).update(ctx, |settings, ctx| {
                    let mut config = settings.next_command.value().clone();
                    config.enabled = *enabled;
                    report_if_error!(settings.next_command.set_value(config, ctx));
                });
            }
            LocalAISettingsPageAction::SetProvider(provider) => {
                LocalAISettings::handle(ctx).update(ctx, |settings, ctx| {
                    let mut config = settings.next_command.value().clone();
                    config.active_provider = *provider;
                    report_if_error!(settings.next_command.set_value(config, ctx));
                });
            }
        }
        ctx.notify();
    }
}

impl View for LocalAISettingsPageView {
    fn ui_name() -> &'static str {
        "LocalAISettingsPage"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        self.page.render(self, app)
    }
}

impl SettingsPageMeta for LocalAISettingsPageView {
    fn section() -> SettingsSection {
        SettingsSection::LocalAI
    }

    fn should_render(&self, _: &AppContext) -> bool {
        true
    }

    fn update_filter(&mut self, query: &str, ctx: &mut ViewContext<Self>) -> MatchData {
        self.page.update_filter(query, ctx)
    }

    fn scroll_to_widget(&mut self, widget_id: &'static str) {
        self.page.scroll_to_widget(widget_id)
    }

    fn clear_highlighted_widget(&mut self) {
        self.page.clear_highlighted_widget();
    }
}

impl From<ViewHandle<LocalAISettingsPageView>> for SettingsPageViewHandle {
    fn from(view_handle: ViewHandle<LocalAISettingsPageView>) -> Self {
        SettingsPageViewHandle::LocalAI(view_handle)
    }
}

struct NextCommandEnabledWidget;

impl SettingsWidget for NextCommandEnabledWidget {
    type View = LocalAISettingsPageView;

    fn search_terms(&self) -> &str {
        "next command ai suggestions ghost text enabled disabled"
    }

    fn render(
        &self,
        view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        render_body_item::<LocalAISettingsPageAction>(
            "Next Command suggestions".to_owned(),
            None,
            view.local_only_icon("enabled", app),
            ToggleState::Enabled,
            appearance,
            ChildView::new(&view.enabled_dropdown).finish(),
            Some(
                "Show generated commands as ghost text. Suggestions are never run automatically."
                    .to_owned(),
            ),
        )
    }
}

struct ProviderWidget;

impl SettingsWidget for ProviderWidget {
    type View = LocalAISettingsPageView;

    fn search_terms(&self) -> &str {
        "next command ai provider openai openrouter ollama local"
    }

    fn render(
        &self,
        view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        render_body_item::<LocalAISettingsPageAction>(
            "Provider".to_owned(),
            None,
            view.local_only_icon("provider", app),
            ToggleState::Enabled,
            appearance,
            ChildView::new(&view.provider_dropdown).finish(),
            Some("Send bounded shell and command context directly to this provider.".to_owned()),
        )
    }
}

struct EndpointWidget;

impl SettingsWidget for EndpointWidget {
    type View = LocalAISettingsPageView;

    fn search_terms(&self) -> &str {
        "next command ai provider endpoint url base openai openrouter ollama local"
    }

    fn render(
        &self,
        view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        render_body_item::<LocalAISettingsPageAction>(
            "Endpoint".to_owned(),
            None,
            view.local_only_icon("endpoint", app),
            ToggleState::Enabled,
            appearance,
            view.render_editor(view.endpoint_editor.clone(), appearance),
            Some(
                "Provider base URL. Include /v1 when required by an OpenAI-compatible service."
                    .to_owned(),
            ),
        )
    }
}

struct ModelWidget;

impl SettingsWidget for ModelWidget {
    type View = LocalAISettingsPageView;

    fn search_terms(&self) -> &str {
        "next command ai provider model name openai openrouter ollama local"
    }

    fn render(
        &self,
        view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        render_body_item::<LocalAISettingsPageAction>(
            "Model".to_owned(),
            None,
            view.local_only_icon("model", app),
            ToggleState::Enabled,
            appearance,
            view.render_editor(view.model_editor.clone(), appearance),
            Some("Exact model identifier expected by the selected provider.".to_owned()),
        )
    }
}

struct CredentialWidget;

impl SettingsWidget for CredentialWidget {
    type View = LocalAISettingsPageView;

    fn search_terms(&self) -> &str {
        "next command ai provider api key credential secure storage openai openrouter ollama"
    }

    fn render(
        &self,
        view: &Self::View,
        appearance: &Appearance,
        app: &AppContext,
    ) -> Box<dyn Element> {
        render_body_item::<LocalAISettingsPageAction>(
            "API key".to_owned(),
            None,
            view.local_only_icon("credential", app),
            ToggleState::Enabled,
            appearance,
            view.render_editor(view.credential_editor.clone(), appearance),
            Some(
                "Stored only in this device's platform secure storage. OpenRouter requires a key."
                    .to_owned(),
            ),
        )
    }
}
