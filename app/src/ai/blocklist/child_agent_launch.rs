use warpui::{AppContext, EntityId, SingletonEntity as _};

use crate::AIExecutionProfilesModel;
use crate::ai::llms::LLMPreferences;

/// Copies the parent's execution profile and effective base model to a child
/// surface before its first request is sent.
pub fn inherit_child_agent_settings(
    parent_surface_id: EntityId,
    child_surface_id: EntityId,
    ctx: &mut AppContext,
) {
    let parent_profile_id = AIExecutionProfilesModel::as_ref(ctx)
        .active_profile(Some(parent_surface_id), ctx)
        .id()
        .clone();
    AIExecutionProfilesModel::handle(ctx).update(ctx, |profiles, ctx| {
        profiles.set_active_profile(child_surface_id, parent_profile_id, ctx);
    });

    let parent_base_model_id = LLMPreferences::as_ref(ctx)
        .get_active_base_model(ctx, Some(parent_surface_id))
        .id
        .clone();
    LLMPreferences::handle(ctx).update(ctx, |preferences, ctx| {
        preferences.update_preferred_agent_mode_llm(&parent_base_model_id, child_surface_id, ctx);
    });
}
