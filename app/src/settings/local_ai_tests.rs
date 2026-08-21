use std::collections::HashMap;
use std::sync::Mutex;

use settings::{PrivatePreferences, PublicPreferences, Setting as _, SettingsManager, SyncToCloud};
use warp_local_ai::{LocalAINextCommandConfig, LocalAIProvider};
use warpui::SingletonEntity as _;
use warpui_extras::secure_storage::{self, AppContextExt as _};
use warpui_extras::user_preferences;

use super::{LocalAICredentials, LocalAINextCommandSetting, LocalAISettings};

#[derive(Default)]
struct InMemorySecureStorage {
    values: Mutex<HashMap<String, String>>,
}

impl secure_storage::SecureStorage for InMemorySecureStorage {
    fn write_value(&self, key: &str, value: &str) -> Result<(), secure_storage::Error> {
        self.values
            .lock()
            .map_err(|err| secure_storage::Error::Unknown(anyhow::anyhow!(err.to_string())))?
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    fn read_value(&self, key: &str) -> Result<String, secure_storage::Error> {
        self.values
            .lock()
            .map_err(|err| secure_storage::Error::Unknown(anyhow::anyhow!(err.to_string())))?
            .get(key)
            .cloned()
            .ok_or(secure_storage::Error::NotFound)
    }

    fn remove_value(&self, key: &str) -> Result<(), secure_storage::Error> {
        self.values
            .lock()
            .map_err(|err| secure_storage::Error::Unknown(anyhow::anyhow!(err.to_string())))?
            .remove(key);
        Ok(())
    }
}

fn register(ctx: &mut warpui::AppContext) {
    ctx.add_singleton_model(|_| {
        PublicPreferences::new(Box::<user_preferences::in_memory::InMemoryPreferences>::default())
    });
    ctx.add_singleton_model(|_| {
        PrivatePreferences::new(Box::<user_preferences::in_memory::InMemoryPreferences>::default())
    });
    ctx.add_singleton_model(|_| SettingsManager::default());
    ctx.add_singleton_model(|_| -> secure_storage::Model {
        Box::<InMemorySecureStorage>::default()
    });
    LocalAISettings::register(ctx);
    LocalAICredentials::register(ctx);
}

#[test]
fn provider_config_is_local_and_not_private() {
    assert_eq!(
        LocalAINextCommandSetting::sync_to_cloud(),
        SyncToCloud::Never
    );
    assert!(!LocalAINextCommandSetting::is_private());
    assert_eq!(
        LocalAINextCommandSetting::toml_path(),
        Some("local_ai.next_command")
    );
}

#[test]
fn credentials_are_stored_only_in_secure_storage() {
    warpui::App::test((), |mut app| async move {
        app.update(register);

        app.update(|ctx| {
            LocalAISettings::handle(ctx).update(ctx, |settings, ctx| {
                let mut config = settings.next_command.value().clone();
                config.active_provider = LocalAIProvider::OpenRouter;
                config.open_router.model = "example/model".to_owned();
                settings.next_command.set_value(config, ctx)
            })
        })
        .unwrap();

        app.update(|ctx| {
            LocalAICredentials::handle(ctx).update(ctx, |credentials, ctx| {
                credentials.set_credential(
                    LocalAIProvider::OpenRouter,
                    Some("secret-sentinel".to_owned()),
                    ctx,
                )
            })
        })
        .unwrap();

        app.read(|ctx| {
            assert_eq!(
                LocalAICredentials::as_ref(ctx).credential(LocalAIProvider::OpenRouter),
                Some("secret-sentinel")
            );
            let connection = LocalAISettings::as_ref(ctx)
                .provider_connection(ctx)
                .expect("configured provider should be available");
            assert_eq!(connection.provider, LocalAIProvider::OpenRouter);
            assert_eq!(connection.model, "example/model");

            let stored_config = LocalAINextCommandSetting::preferences_for_setting(ctx)
                .read_value(LocalAINextCommandSetting::storage_key())
                .expect("local settings should be readable")
                .expect("local provider config should be stored");
            assert!(stored_config.contains("example/model"));
            assert!(!stored_config.contains("secret-sentinel"));

            let config_json = serde_json::to_string(&LocalAINextCommandConfig::default()).unwrap();
            assert!(!config_json.contains("secret-sentinel"));
            assert!(
                ctx.secure_storage()
                    .read_value("WarpLocalAINextCommandOpenRouterCredential")
                    .is_ok_and(|value| value == "secret-sentinel")
            );
        });
    });
}
