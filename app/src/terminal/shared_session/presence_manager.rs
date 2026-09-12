use std::collections::{HashMap, HashSet};
use std::iter;

use asset_cache::AssetCacheExt as _;
use futures::future::BoxFuture;
use futures_util::future::join_all;
use itertools::{Either, Itertools};
use pathfinder_color::ColorU;
use rand::Rng;
#[cfg(not(target_arch = "wasm32"))]
use session_sharing_protocol::common::Viewer;
use session_sharing_protocol::common::{
    InputReplicaId, ParticipantId, ParticipantInfo, ParticipantList, ParticipantPresenceUpdate,
    PresenceUpdate, Role, Selection,
};
use warpui::assets::asset_cache::{AssetCache, AssetState};
use warpui::r#async::SpawnedFutureHandle;
use warpui::image_cache::ImageType;
use warpui::{AppContext, Entity, ModelContext, SingletonEntity};

use crate::auth::UserUid;

/// A set of pre-assigned colors that we use for shared session participants.
/// These come from https://www.figma.com/file/chk9pwt35jTJhf9KnHmZyE/Components?type=design&node-id=1650-1410&mode=design&t=RTHbE9G6NLhFRqLQ-0.
const PRESET_COLORS: &[ColorU] = &[
    ColorU {
        r: 93,
        g: 202,
        b: 60,
        a: 255,
    },
    ColorU {
        r: 174,
        g: 67,
        b: 255,
        a: 255,
    },
    ColorU {
        r: 224,
        g: 222,
        b: 19,
        a: 255,
    },
    ColorU {
        r: 255,
        g: 125,
        b: 38,
        a: 255,
    },
    ColorU {
        r: 68,
        g: 233,
        b: 237,
        a: 255,
    },
    ColorU {
        r: 54,
        g: 98,
        b: 236,
        a: 255,
    },
    ColorU {
        r: 255,
        g: 13,
        b: 226,
        a: 255,
    },
];

/// Helper struct containing participant info and anything else necessary for rendering
/// for a present participant.
#[derive(Clone)]
pub struct Participant {
    pub info: ParticipantInfo,

    /// The color assigned to this participant
    pub color: ColorU,

    /// Is None iff participant is sharer.
    pub role: Option<Role>,
}

impl Participant {
    pub fn id(&self) -> &ParticipantId {
        &self.info.id
    }

    pub fn input_replica_id(&self) -> &InputReplicaId {
        &self.info.profile_data.input_replica_id
    }
}

/// A viewer who was once part of the session
/// but no longer is.
#[derive(Clone)]
pub struct AbsentViewer {
    /// The last known info we had about the viewer.
    participant_info: ParticipantInfo,
}

impl AbsentViewer {
    pub fn id(&self) -> &ParticipantId {
        &self.participant_info.id
    }

    pub fn input_replica_id(&self) -> &InputReplicaId {
        &self.participant_info.profile_data.input_replica_id
    }
}

/// Manager for assigning colors to shared session participants as they join and leave.
/// This should contain the data needed to render presence-related UIs.
/// The presence manager does not store participant data about ourselves, whether we are the sharer or viewer.
pub struct PresenceManager {
    /// Our own Participant ID.
    id: ParticipantId,

    /// Our own Firebase UID.
    firebase_uid: UserUid,

    /// Our own role, None iff is sharer.
    pub role: Option<Role>,

    /// Participant ID of the sharer.
    sharer_id: ParticipantId,

    /// Is None iff we ourselves are the sharer.
    sharer: Option<Participant>,

    /// The set of viewers who are still part of the session.
    ///
    /// If we are ourselves a viewer, this map does _not_ include our own state.
    present_viewers: HashMap<ParticipantId, Participant>,

    /// The set of viewers who were once part of the session but no longer are.
    /// By default, all of the `get_*` APIs that return a list of participants
    /// _do not_ include the absent viewers.
    absent_viewers: HashMap<ParticipantId, AbsentViewer>,

    chosen_colors: HashSet<ColorU>,

    /// Loading participants is a future because we may need to download an image.
    /// Note even if there is no image, the participant is still loaded as a future.
    load_participants_imgs_future_handle: Option<SpawnedFutureHandle>,

    /// Whether we ourselves are attempting to reconnect to the server.
    /// If this is true, all avatars should have a muted color.
    is_reconnecting: bool,
}

/// Returns the first available preset color, or a random color if all are taken.
pub fn get_available_color(chosen_colors: &HashSet<ColorU>) -> ColorU {
    for color in PRESET_COLORS {
        if !chosen_colors.contains(color) {
            return *color;
        }
    }
    // If we ran out of colors, generate a random one.
    ColorU::new(
        rand::thread_rng().gen_range(0..=255),
        rand::thread_rng().gen_range(0..=255),
        rand::thread_rng().gen_range(0..=255),
        255,
    )
}

impl PresenceManager {
    pub fn new_for_viewer(
        id: ParticipantId,
        firebase_uid: UserUid,
        participants: ParticipantList,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        // Populate sharer info, remaining fields for sharer and viewer
        // will be populated in the call to `update_participants`.
        let mut chosen_colors = HashSet::new();
        let color = get_available_color(&chosen_colors);
        chosen_colors.insert(color);

        let sharer = Participant {
            info: participants.sharer.info.clone(),
            color,
            role: None,
        };

        let mut manager = Self {
            id,
            firebase_uid,
            role: Some(Role::default()),
            sharer_id: participants.sharer.info.id.clone(),
            sharer: Some(sharer),
            present_viewers: HashMap::new(),
            absent_viewers: HashMap::new(),
            chosen_colors,
            load_participants_imgs_future_handle: None,
            is_reconnecting: false,
        };
        manager.update_participants(participants, ctx);
        manager
    }

    /// Returns our own participant id.
    pub fn id(&self) -> ParticipantId {
        self.id.clone()
    }

    /// Returns our own Firebase UID.
    pub fn firebase_uid(&self) -> UserUid {
        self.firebase_uid
    }

    /// Returns the sharer's participant id.
    pub fn sharer_id(&self) -> ParticipantId {
        self.sharer_id.clone()
    }

    /// Returns our own role, `None` iff we are the sharer.
    pub fn role(&self) -> Option<Role> {
        self.role
    }

    /// Returns the viewer's role, if the viewer is known to us.
    pub fn viewer_role(&self, viewer_id: &ParticipantId) -> Option<Role> {
        self.present_viewers.get(viewer_id).and_then(|v| v.role)
    }

    /// Returns the present viewers of this shared session, not including ourselves.
    /// There is no guarantee of the ordering of viewers, so callers should sort by ID for a stable ordering.
    pub fn get_present_viewers(&self) -> impl Iterator<Item = &Participant> {
        self.present_viewers.values()
    }

    /// Returns the sharer of this shared session.
    /// Returns None if we are the sharer ourselves (we should not need presence data for ourselves).
    pub fn get_sharer(&self) -> Option<&Participant> {
        self.sharer.as_ref()
    }

    /// Returns all present participants of this shared session, including sharer and viewers,
    /// but not including ourselves.
    pub fn all_present_participants(&self) -> impl Iterator<Item = &Participant> {
        if let Some(sharer) = self.get_sharer() {
            return Either::Left(iter::once(sharer).chain(self.get_present_viewers()));
        }
        Either::Right(self.get_present_viewers())
    }

    /// Returns the participant identified by id iff the participant is present.
    pub fn get_participant(&self, id: &ParticipantId) -> Option<&Participant> {
        if let Some(viewer) = self.present_viewers.get(id) {
            return Some(viewer);
        } else if let Some(sharer) = self.sharer.as_ref()
            && self.sharer_id == *id
        {
            return Some(sharer);
        }
        None
    }

    pub fn update_participants(
        &mut self,
        participants: ParticipantList,
        ctx: &mut ModelContext<Self>,
    ) {
        // If there was a previous in-flight future updating participants, cancel it since our new list is more up to date.
        if let Some(old_abort_handle) = self.load_participants_imgs_future_handle.take() {
            old_abort_handle.abort();
        }

        // The new or updated participants.
        let mut latest_participants = Vec::new();

        // A list of futures. Each one represents a profile image that's being loaded for a participant.
        let mut participant_image_loading_futures = Vec::new();

        // Update sharer info
        let incoming_sharer_info = participants.sharer.info;
        if let Some(sharer) = self.sharer.as_mut() {
            sharer.info = incoming_sharer_info.clone();

            if let Some(future) = Self::when_profile_image_is_loaded(sharer, ctx) {
                participant_image_loading_futures.push(future);
            }
            latest_participants.push(sharer.clone());
        }

        for viewer in participants.viewers {
            if !viewer.is_present {
                if let Some(viewer) = self.present_viewers.remove(&viewer.info.id) {
                    self.chosen_colors.remove(&viewer.color);
                }
                self.absent_viewers.insert(
                    viewer.info.id.clone(),
                    AbsentViewer {
                        participant_info: viewer.info,
                    },
                );
                continue;
            }

            let info = viewer.info;
            // Only store role data for ourselves.
            if info.id == self.id {
                self.role = Some(viewer.role);
                continue;
            }

            // If this participant already existed, update the info and role
            // while keeping their color.
            if let Some(existing_participant) = self.present_viewers.get_mut(&info.id) {
                existing_participant.info = info;
                existing_participant.role = Some(viewer.role);
                continue;
            };

            // Otherwise, pick an available color and add them.
            let color = get_available_color(&self.chosen_colors);
            self.chosen_colors.insert(color);

            let new_viewer = Participant {
                info,
                color,
                role: Some(viewer.role),
            };

            if let Some(future) = Self::when_profile_image_is_loaded(&new_viewer, ctx) {
                participant_image_loading_futures.push(future);
            }
            latest_participants.push(new_viewer);
        }

        // Spawn a future that waits for all the new profile images to be loaded into memory.
        let load_participants_future_handle = ctx.spawn(
            async move {
                join_all(participant_image_loading_futures).await;
            },
            |manager, _, ctx| {
                manager.on_participant_images_loaded(latest_participants, ctx);
            },
        );
        self.load_participants_imgs_future_handle = Some(load_participants_future_handle.clone());
    }

    /// Returns a future that resolves when the participant's profile image is loaded. If the participant
    /// doesn't have a profile image OR their image is already available in memory, returns None.
    fn when_profile_image_is_loaded(
        participant: &Participant,
        app: &AppContext,
    ) -> Option<BoxFuture<'static, ()>> {
        let url = participant.info.profile_data.photo_url.as_ref()?;
        let asset_cache = AssetCache::as_ref(app);

        // Make a non-blocking check to see if the image is loaded yet. If the image hasn't been seen
        // before, this call spawns a task to fetch the bytes and parse it into an image.
        match asset_cache.load_asset_from_url::<ImageType>(url, None) {
            AssetState::Loading { handle } => handle.when_loaded(asset_cache),
            _ => None,
        }
    }

    pub fn update_participant_presence(
        &mut self,
        update: ParticipantPresenceUpdate,
        _ctx: &mut ModelContext<Self>,
    ) {
        let participant = if self.sharer_id == update.participant_id {
            self.sharer.as_mut()
        } else {
            self.present_viewers.get_mut(&update.participant_id)
        };

        let Some(participant) = participant else {
            if self.id != update.participant_id {
                log::warn!(
                    "Received shared session participant presence update for participant that doesn't exist"
                );
            }
            return;
        };
        let PresenceUpdate::Selection(selection) = update.update;

        // Selection info is needed for rendering remote cursors in input
        participant.info.selection = selection;
    }

    pub fn update_participant_role(
        &mut self,
        participant_id: &ParticipantId,
        role: Role,
        _ctx: &mut ModelContext<Self>,
    ) {
        if participant_id == &self.id {
            self.role = Some(role);
        } else {
            let Some(participant) = self.present_viewers.get_mut(participant_id) else {
                log::warn!(
                    "Received shared session participant role update for participant that doesn't exist"
                );
                return;
            };
            participant.role = Some(role);
        }
    }

    pub fn set_is_reconnecting(
        &mut self,
        is_self_reconnecting: bool,
        _ctx: &mut ModelContext<Self>,
    ) {
        self.is_reconnecting = is_self_reconnecting;
    }

    pub fn is_reconnecting(&self) -> bool {
        self.is_reconnecting
    }

    fn on_participant_images_loaded(
        &mut self,
        latest_participants: Vec<Participant>,
        ctx: &mut ModelContext<Self>,
    ) {
        // Once all participant futures have completed, update the participant list and emit an event.
        for participant in latest_participants {
            if participant.info.id == self.sharer_id {
                self.sharer = Some(participant);
            } else {
                self.present_viewers
                    .insert(participant.info.id.clone(), participant);
            }
        }
        ctx.emit(Event::ParticipantListUpdated);
    }

    pub fn absent_viewers(&self) -> impl Iterator<Item = &AbsentViewer> + '_ {
        self.absent_viewers.values()
    }

    /// Returns a viewer's firebase uid, if the viewer is known to us.
    pub fn viewer_firebase_uid(&self, viewer_id: &ParticipantId) -> Option<UserUid> {
        if *viewer_id == self.id {
            return Some(self.firebase_uid);
        }

        self.present_viewers
            .get(viewer_id)
            .map(|v| v.info.profile_data.firebase_uid.as_str())
            .or_else(|| {
                self.absent_viewers
                    .get(viewer_id)
                    .map(|v| v.participant_info.profile_data.firebase_uid.as_str())
            })
            .map(UserUid::new)
    }

    /// Returns all of the present viewer IDs associated with the given Firebase
    /// UID, including ourselves if applicable.
    pub fn present_viewer_ids_for_uid(
        &self,
        viewer_uid: UserUid,
    ) -> impl Iterator<Item = &ParticipantId> + '_ {
        let is_viewer_self = self.firebase_uid == viewer_uid;
        let viewer_ids = self
            .present_viewers
            .values()
            .filter(move |v| viewer_uid.as_string() == v.info.profile_data.firebase_uid)
            .map(|v| &v.info.id);
        viewer_ids.chain(is_viewer_self.then_some(&self.id))
    }

    /// Returns a participant ID for a participant associated with the given
    /// Firebase UID.
    pub fn present_viewer_id_for_uid(&self, viewer_uid: UserUid) -> Option<&ParticipantId> {
        self.present_viewer_ids_for_uid(viewer_uid).next()
    }

    /// Returns the only distinct present viewer UID. Multiple present viewers
    /// with the same UID count as one user.
    pub fn single_distinct_present_viewer_uid(&self) -> Option<&str> {
        Self::single_distinct_uid(
            self.get_present_viewers()
                .map(|v| v.info.profile_data.firebase_uid.as_str()),
        )
    }

    /// Like `single_distinct_present_viewer_uid`, but reads directly from a
    /// participant list before the presence manager finishes processing it.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn single_distinct_present_viewer_uid_from_viewers<'a>(
        viewers: impl Iterator<Item = &'a Viewer>,
    ) -> Option<&'a str> {
        Self::single_distinct_uid(
            viewers
                .filter(|v| v.is_present)
                .map(|v| v.info.profile_data.firebase_uid.as_str()),
        )
    }

    fn single_distinct_uid<'a>(mut uids: impl Iterator<Item = &'a str>) -> Option<&'a str> {
        let uid = uids.next()?;
        uids.all(|other_uid| other_uid == uid).then_some(uid)
    }
}

pub enum Event {
    ParticipantListUpdated,
}

impl Entity for PresenceManager {
    type Event = Event;
}

#[cfg(test)]
#[path = "presence_manager_tests.rs"]
mod tests;
