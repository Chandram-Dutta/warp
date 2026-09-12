use settings::Setting;
use warp_core::features::FeatureFlag;
use warp_errors::report_if_error;
use warpui::{Element, Entity, SingletonEntity, TypedActionView, View, ViewContext, ViewHandle};

use crate::appearance::Appearance;
use crate::settings_view::settings_page::render_dropdown_item;
use crate::util::file::external_editor::settings::EditorChoice;
use crate::util::file::external_editor::{EditorSettings, SUPPORTED_EDITORS};
use crate::view_components::{Dropdown, DropdownItem};

#[derive(Debug, Clone, PartialEq)]
pub enum ExternalEditorAction {
    SetEditor(EditorChoice),
}

pub struct ExternalEditorView {
    editor_dropdown: ViewHandle<Dropdown<ExternalEditorAction>>,
}

impl ExternalEditorView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let editor_to_open_files = *EditorSettings::as_ref(ctx).open_file_editor;
        let editor_dropdown = ctx.add_typed_action_view(|ctx| {
            let mut dropdown = Dropdown::new(ctx);
            Self::init_editor_dropdown(&editor_to_open_files, &mut dropdown, ctx);
            dropdown
        });
        ctx.subscribe_to_model(&EditorSettings::handle(ctx), |me, settings, _, ctx| {
            let editor = *settings.as_ref(ctx).open_file_editor;
            me.editor_dropdown.update(ctx, |dropdown, ctx| {
                Self::init_editor_dropdown(&editor, dropdown, ctx);
            });
            ctx.notify();
        });
        Self { editor_dropdown }
    }

    fn init_editor_dropdown(
        editor_to_open_files: &EditorChoice,
        dropdown: &mut Dropdown<ExternalEditorAction>,
        ctx: &mut ViewContext<Dropdown<ExternalEditorAction>>,
    ) {
        let default_option_text = "Default App";
        let mut items = vec![DropdownItem::new(
            default_option_text,
            ExternalEditorAction::SetEditor(EditorChoice::SystemDefault),
        )];
        if FeatureFlag::AllowOpeningFileLinksUsingEditorEnv.is_enabled() {
            items.push(DropdownItem::new(
                "$EDITOR",
                ExternalEditorAction::SetEditor(EditorChoice::EnvEditor),
            ));
        }
        for editor in SUPPORTED_EDITORS {
            if editor.is_installed(ctx) {
                items.push(DropdownItem::new(
                    format!("{editor}"),
                    ExternalEditorAction::SetEditor(EditorChoice::ExternalEditor(*editor)),
                ));
            }
        }
        dropdown.set_items(items, ctx);
        match editor_to_open_files {
            EditorChoice::ExternalEditor(editor) => {
                dropdown.set_selected_by_name(format!("{editor}"), ctx)
            }
            EditorChoice::EnvEditor => dropdown.set_selected_by_name("$EDITOR", ctx),
            EditorChoice::SystemDefault => dropdown.set_selected_by_name(default_option_text, ctx),
        };
    }
}

impl Entity for ExternalEditorView {
    type Event = ();
}

impl View for ExternalEditorView {
    fn ui_name() -> &'static str {
        "ExternalEditorView"
    }

    fn render(&self, app: &warpui::AppContext) -> Box<dyn Element> {
        render_dropdown_item(
            Appearance::as_ref(app),
            "Choose an editor to open file links",
            None,
            None,
            None,
            &self.editor_dropdown,
        )
    }
}

impl TypedActionView for ExternalEditorView {
    type Action = ExternalEditorAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            ExternalEditorAction::SetEditor(editor) => {
                EditorSettings::handle(ctx).update(ctx, |settings, ctx| {
                    report_if_error!(settings.open_file_editor.set_value(*editor, ctx));
                });
            }
        }
    }
}
