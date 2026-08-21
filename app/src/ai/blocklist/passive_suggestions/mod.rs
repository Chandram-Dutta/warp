#[cfg(not(feature = "local_only"))]
mod legacy;
#[cfg(feature = "local_only")]
mod legacy_local_only;
#[cfg(not(feature = "local_only"))]
mod maa;
#[cfg(feature = "local_only")]
mod maa_local_only;
#[cfg(not(feature = "local_only"))]
mod static_prompt_suggestions;

#[cfg(not(feature = "local_only"))]
pub use legacy::{
    PassiveSuggestionsEvent as LegacyPassiveSuggestionsEvent,
    PassiveSuggestionsModel as LegacyPassiveSuggestionsModel,
};
#[cfg(feature = "local_only")]
pub use legacy_local_only::{
    PassiveSuggestionsEvent as LegacyPassiveSuggestionsEvent,
    PassiveSuggestionsModel as LegacyPassiveSuggestionsModel,
};
#[cfg(not(feature = "local_only"))]
pub use maa::{
    PassiveSuggestionsEvent as MaaPassiveSuggestionsEvent,
    PassiveSuggestionsModel as MaaPassiveSuggestionsModel,
};
#[cfg(feature = "local_only")]
pub use maa_local_only::{
    PassiveSuggestionsEvent as MaaPassiveSuggestionsEvent,
    PassiveSuggestionsModel as MaaPassiveSuggestionsModel,
};
use warpui::ModelHandle;

#[derive(Clone)]
pub struct PassiveSuggestionsModels {
    pub legacy: ModelHandle<LegacyPassiveSuggestionsModel>,
    pub maa: ModelHandle<MaaPassiveSuggestionsModel>,
}
